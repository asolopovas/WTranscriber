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
    audio::GenericAudioBufferRef,
    codecs::audio::{AudioDecoderOptions, well_known::CODEC_ID_OPUS},
    errors::Error as SymphoniaError,
    formats::{FormatOptions, FormatReader, TrackType, probe::Hint},
    io::{MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
};

use crate::{
    audio::wav::{WHISPER_SAMPLE_RATE, write_pcm16_wav},
    error::{Error, Result},
};

fn open_audio(input: &Path) -> Result<Box<dyn FormatReader>> {
    let file = File::open(input)
        .map_err(|e| Error::Transcribe(format!("open {}: {e}", input.display())))?;
    let mss = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());
    let mut hint = Hint::new();
    if let Some(ext) = input.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| Error::Transcribe(format!("probe: {e}")))
}

pub fn probe_duration_ms(input: &Path) -> Option<u64> {
    let mut format = open_audio(input).ok()?;
    let track = format.default_track(TrackType::Audio)?;
    let track_id = track.id;
    let time_base = track.time_base?;
    if let Some(duration) = track.duration {
        return u64::try_from(time_base.calc_duration(duration)?.as_millis()).ok();
    }
    let mut end = track.start_ts;
    let start = track.start_ts;
    while let Ok(Some(packet)) = format.next_packet() {
        if packet.track_id == track_id {
            end = end.max(packet.pts.saturating_add(packet.dur));
        }
    }
    let elapsed = time_base.calc_time(end)?.as_millis() - time_base.calc_time(start)?.as_millis();
    (elapsed > 0).then(|| u64::try_from(elapsed).ok()).flatten()
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
    let mut format = open_audio(input)?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| Error::Transcribe("no default track".into()))?;
    let track_id = track.id;
    let delay = track.delay;
    let codec_params = track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio())
        .ok_or_else(|| Error::Transcribe("missing audio codec parameters".into()))?;
    let sample_rate = codec_params
        .sample_rate
        .ok_or_else(|| Error::Transcribe("missing sample rate".into()))?;
    let channels = codec_params
        .channels
        .as_ref()
        .map_or(1, symphonia::core::audio::Channels::count)
        .max(1);

    if codec_params.codec == CODEC_ID_OPUS {
        return decode_opus_to_mono_f32(format, track_id, channels, delay);
    }

    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(codec_params, &AudioDecoderOptions::default())
        .map_err(|e| Error::Transcribe(format!("decoder: {e}")))?;

    let mut samples: Vec<f32> = Vec::new();
    let mut interleaved = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) | Err(SymphoniaError::ResetRequired) => break,
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(e) => return Err(Error::Transcribe(format!("packet: {e}"))),
        };
        if packet.track_id != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(buf) => append_samples(&mut samples, &buf, &mut interleaved),
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
            Ok(Some(p)) => p,
            Ok(None) | Err(SymphoniaError::ResetRequired) => break,
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(e) => return Err(Error::Transcribe(format!("opus packet: {e}"))),
        };
        if packet.track_id != track_id {
            continue;
        }

        let frames = decoder
            .decode_float(&packet.data, &mut pcm, false)
            .map_err(|e| Error::Transcribe(format!("opus decode: {e}")))?;
        let trim_start = pending_skip.saturating_add(packet.trim_start.get() as usize);
        pending_skip = 0;
        let trim_end = packet.trim_end.get() as usize;
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

fn append_samples(out: &mut Vec<f32>, buf: &GenericAudioBufferRef<'_>, interleaved: &mut Vec<f32>) {
    let channels = buf.num_planes();
    if channels == 0 {
        return;
    }
    interleaved.resize(buf.samples_interleaved(), 0.0);
    buf.copy_to_slice_interleaved(interleaved.as_mut_slice());
    out.extend(
        interleaved
            .chunks_exact(channels)
            .map(|frame| frame.iter().copied().sum::<f32>() / channels as f32),
    );
}

fn resample(input: &[f32], from: u32, to: u32) -> Result<Vec<f32>> {
    if input.is_empty() {
        return Ok(Vec::new());
    }
    let ratio = to as f64 / from as f64;
    let chunk = 1024;
    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: Some(0.95),
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
    fn probes_and_decodes_pcm_audio() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.wav");
        let input = vec![0.25_f32; 16_000];
        write_pcm16_wav(&path, &input, 16_000).unwrap();
        assert_eq!(probe_duration_ms(&path), Some(1_000));
        let (decoded, rate) = decode_to_mono_f32(&path).unwrap();
        assert_eq!(rate, 16_000);
        assert_eq!(decoded.len(), input.len());
        assert!((decoded[8_000] - input[8_000]).abs() < 0.001);
    }

    #[test]
    fn decodes_m4a_with_custom_null_and_mp4_sl_descriptors() {
        let fixture = include_bytes!("fixtures/aac-sl-config.m4a");
        let descriptor = [6, 128, 128, 128, 1, 2];
        let offset = fixture
            .windows(descriptor.len())
            .position(|window| window == descriptor)
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("voice-note.m4a");
        for predefined in [0, 1, 2] {
            let mut audio = fixture.to_vec();
            audio[offset + descriptor.len() - 1] = predefined;
            std::fs::write(&path, audio).unwrap();
            assert!(probe_duration_ms(&path).is_some());
            let (samples, rate) = decode_to_mono_f32(&path).unwrap();
            assert_eq!(rate, 16_000);
            assert!(samples.len() >= 1_600);
            assert!(samples.iter().all(|sample| sample.is_finite()));
        }
    }

    #[test]
    fn decodes_opus_and_resamples_without_external_tools() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.ogg");
        std::fs::write(&path, include_bytes!("fixtures/opus.ogg")).unwrap();
        let (samples, rate) = decode_to_mono_f32(&path).unwrap();
        assert_eq!(rate, 48_000);
        assert!(samples.len() >= 4_800);
        assert!(samples.iter().all(|sample| sample.is_finite()));
        let resampled = decode_to_pcm_f32(&path, 16_000).unwrap();
        assert!(resampled.len().abs_diff(1_600) < 100);
    }

    #[test]
    fn resample_empty_returns_empty() {
        assert_eq!(resample(&[], 44_100, 16_000).unwrap(), Vec::<f32>::new());
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
