#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_lossless
)]

use std::{fs::File, path::Path};

use rubato::audioadapter_buffers::direct::SequentialSlice;
use rubato::{
    Async, FixedAsync, Indexing, Resampler, SincInterpolationParameters, SincInterpolationType,
    WindowFunction,
};
use symphonia::core::{
    audio::{AudioBufferRef, Signal},
    codecs::{CODEC_TYPE_OPUS, DecoderOptions},
    errors::Error as SymphoniaError,
    formats::{FormatOptions, FormatReader},
    io::{MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
    probe::Hint,
};

use crate::{
    audio::wav::{WHISPER_SAMPLE_RATE, write_pcm16_wav},
    error::{Error, Result},
};

pub fn probe_duration_ms(input: &Path) -> Option<u64> {
    let file = File::open(input).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());
    let mut hint = Hint::new();
    if let Some(ext) = input.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .ok()?;
    let track = probed.format.default_track()?;
    let sr = track.codec_params.sample_rate? as u64;
    if let Some(frames) = track.codec_params.n_frames
        && sr > 0
    {
        return Some(frames * 1000 / sr);
    }
    if let Some(tb) = track.codec_params.time_base
        && let Some(n_ts) = track.codec_params.n_frames
    {
        let secs = (n_ts as f64) * (tb.numer as f64) / (tb.denom as f64);
        return Some((secs * 1000.0) as u64);
    }
    None
}

pub fn decode_to_wav(input: &Path, output: &Path) -> Result<()> {
    let (samples, sr) = decode_to_mono_f32(input)?;
    let resampled = if sr == WHISPER_SAMPLE_RATE {
        samples
    } else {
        resample(&samples, sr, WHISPER_SAMPLE_RATE)?
    };
    write_pcm16_wav(output, &resampled, WHISPER_SAMPLE_RATE)
}

pub fn decode_to_pcm_f32(input: &Path, target_sr: i32) -> Result<Vec<f32>> {
    let (samples, sr) = decode_to_mono_f32(input)?;
    let target = u32::try_from(target_sr).unwrap_or(WHISPER_SAMPLE_RATE);
    if sr == target {
        Ok(samples)
    } else {
        resample(&samples, sr, target)
    }
}

fn decode_to_mono_f32(input: &Path) -> Result<(Vec<f32>, u32)> {
    let file = File::open(input)
        .map_err(|e| Error::Transcribe(format!("open {}: {e}", input.display())))?;
    let mss = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());

    let mut hint = Hint::new();
    if let Some(ext) = input.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| Error::Transcribe(format!("probe: {e}")))?;

    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| Error::Transcribe("no default track".into()))?;
    let track_id = track.id;
    let codec_params = track.codec_params.clone();
    let sample_rate = codec_params
        .sample_rate
        .ok_or_else(|| Error::Transcribe("missing sample rate".into()))?;
    let channels = codec_params
        .channels
        .map_or(1, symphonia::core::audio::Channels::count)
        .max(1);

    if codec_params.codec == CODEC_TYPE_OPUS {
        return decode_opus_to_mono_f32(format, track_id, channels, codec_params.delay);
    }

    let mut decoder = symphonia::default::get_codecs()
        .make(&codec_params, &DecoderOptions::default())
        .map_err(|e| Error::Transcribe(format!("decoder: {e}")))?;

    let mut samples: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(SymphoniaError::ResetRequired) => break,
            Err(e) => return Err(Error::Transcribe(format!("packet: {e}"))),
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(buf) => append_samples(&mut samples, buf, channels),
            Err(SymphoniaError::DecodeError(_)) => {}
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(e) => return Err(Error::Transcribe(format!("decode: {e}"))),
        }
    }

    Ok((samples, sample_rate))
}

fn decode_opus_to_mono_f32(
    mut format: Box<dyn FormatReader>,
    track_id: u32,
    channels: usize,
    delay: Option<u32>,
) -> Result<(Vec<f32>, u32)> {
    let channels = channels.clamp(1, 2);
    crate::logfile::info(&format!(
        "audio decoder: using built-in opus decoder channels={channels}"
    ));
    let mut decoder = opus_decoder::OpusDecoder::new(48_000, channels)
        .map_err(|e| Error::Transcribe(format!("opus decoder: {e}")))?;
    let mut pcm = vec![0.0_f32; decoder.max_frame_size_per_channel() * channels];
    let mut samples = Vec::<f32>::new();
    let mut pending_skip = delay.unwrap_or_default() as usize;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(SymphoniaError::ResetRequired) => break,
            Err(e) => return Err(Error::Transcribe(format!("opus packet: {e}"))),
        };
        if packet.track_id() != track_id {
            continue;
        }

        let frames = decoder
            .decode_float(&packet.data, &mut pcm, false)
            .map_err(|e| Error::Transcribe(format!("opus decode: {e}")))?;
        let trim_start = pending_skip.saturating_add(packet.trim_start as usize);
        pending_skip = 0;
        let trim_end = packet.trim_end as usize;
        if trim_start >= frames {
            continue;
        }
        let end = frames.saturating_sub(trim_end);
        if trim_start >= end {
            continue;
        }
        for frame in trim_start..end {
            let offset = frame * channels;
            let sum = pcm[offset..offset + channels].iter().copied().sum::<f32>();
            samples.push(sum / channels as f32);
        }
    }

    Ok((samples, 48_000))
}

fn append_samples(out: &mut Vec<f32>, buf: AudioBufferRef<'_>, channels: usize) {
    macro_rules! mix {
        ($buf:expr, $convert:expr) => {{
            let frames = $buf.frames();
            for i in 0..frames {
                let mut sum = 0.0_f32;
                for ch in 0..channels {
                    sum += $convert($buf.chan(ch)[i]);
                }
                out.push(sum / channels as f32);
            }
        }};
    }
    match buf {
        AudioBufferRef::F32(b) => mix!(b, |s: f32| s),
        AudioBufferRef::F64(b) => mix!(b, |s: f64| s as f32),
        AudioBufferRef::U8(b) => mix!(b, |s: u8| (s as f32 - 128.0) / 128.0),
        AudioBufferRef::U16(b) => mix!(b, |s: u16| (s as f32 - 32768.0) / 32768.0),
        AudioBufferRef::U24(b) => mix!(b, |s: symphonia::core::sample::u24| {
            (s.inner() as f32 - 8_388_608.0) / 8_388_608.0
        }),
        AudioBufferRef::U32(b) => mix!(b, |s: u32| (s as f64 - 2_147_483_648.0) as f32
            / 2_147_483_648.0),
        AudioBufferRef::S8(b) => mix!(b, |s: i8| s as f32 / 128.0),
        AudioBufferRef::S16(b) => mix!(b, |s: i16| s as f32 / 32768.0),
        AudioBufferRef::S24(b) => mix!(b, |s: symphonia::core::sample::i24| s.inner() as f32
            / 8_388_608.0),
        AudioBufferRef::S32(b) => mix!(b, |s: i32| s as f32 / 2_147_483_648.0),
    }
}

fn resample(input: &[f32], from: u32, to: u32) -> Result<Vec<f32>> {
    if input.is_empty() {
        return Ok(Vec::new());
    }
    let ratio = to as f64 / from as f64;
    let chunk = 1024;
    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };
    let mut resampler = Async::<f32>::new_sinc(ratio, 2.0, &params, chunk, 1, FixedAsync::Input)
        .map_err(|e| Error::Transcribe(format!("resampler: {e}")))?;

    let chunk_out_max = resampler.output_frames_max();
    let mut chunk_in_buf = vec![0.0_f32; chunk];
    let mut chunk_out_buf = vec![0.0_f32; chunk_out_max];
    let mut out: Vec<f32> = Vec::with_capacity((input.len() as f64 * ratio) as usize + chunk);
    let mut pos = 0;
    while pos + chunk <= input.len() {
        chunk_in_buf.copy_from_slice(&input[pos..pos + chunk]);
        let n_out = run_chunk(
            &mut resampler,
            &chunk_in_buf,
            &mut chunk_out_buf,
            chunk,
            None,
        )?;
        out.extend_from_slice(&chunk_out_buf[..n_out]);
        pos += chunk;
    }
    if pos < input.len() {
        let partial_len = input.len() - pos;
        chunk_in_buf.fill(0.0);
        chunk_in_buf[..partial_len].copy_from_slice(&input[pos..]);
        let indexing = Indexing {
            input_offset: 0,
            output_offset: 0,
            active_channels_mask: None,
            partial_len: Some(partial_len),
        };
        let n_out = run_chunk(
            &mut resampler,
            &chunk_in_buf,
            &mut chunk_out_buf,
            chunk,
            Some(&indexing),
        )?;
        let keep = ((partial_len as f64) * ratio) as usize;
        out.extend_from_slice(&chunk_out_buf[..n_out.min(keep)]);
    }
    Ok(out)
}

fn run_chunk(
    resampler: &mut Async<f32>,
    in_buf: &[f32],
    out_buf: &mut [f32],
    chunk: usize,
    indexing: Option<&Indexing>,
) -> Result<usize> {
    let input = SequentialSlice::new(in_buf, 1, chunk)
        .map_err(|e| Error::Transcribe(format!("resample input adapter: {e}")))?;
    let chunk_out_max = out_buf.len();
    let mut output = SequentialSlice::new_mut(out_buf, 1, chunk_out_max)
        .map_err(|e| Error::Transcribe(format!("resample output adapter: {e}")))?;
    let (_, n_out) = resampler
        .process_into_buffer(&input, &mut output, indexing)
        .map_err(|e| Error::Transcribe(format!("resample: {e}")))?;
    Ok(n_out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_empty_returns_empty() {
        assert!(resample(&[], 44_100, 16_000).unwrap().is_empty());
    }

    #[test]
    fn resample_downsamples_to_expected_length() {
        let input = vec![0.0_f32; 44_100];
        let out = resample(&input, 44_100, 16_000).unwrap();
        let expected = 16_000_usize;
        let actual = out.len();
        assert!(
            actual.abs_diff(expected) < 100,
            "expected ~{expected} samples, got {actual}",
        );
    }

    #[test]
    fn resample_upsamples_to_expected_length() {
        let input = vec![0.0_f32; 16_000];
        let out = resample(&input, 16_000, 48_000).unwrap();
        let expected = 48_000_usize;
        let actual = out.len();
        assert!(
            actual.abs_diff(expected) < 200,
            "expected ~{expected} samples, got {actual}",
        );
    }

    #[test]
    fn resample_preserves_dc_amplitude_within_tolerance() {
        let input = vec![0.5_f32; 4_096];
        let out = resample(&input, 16_000, 22_050).unwrap();
        let mid = out.len() / 2;
        let probe = out[mid];
        assert!(
            (probe - 0.5).abs() < 0.05,
            "expected DC near 0.5, got {probe}",
        );
    }
}
