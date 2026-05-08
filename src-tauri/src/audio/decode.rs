#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_lossless
)]

use std::{fs::File, path::Path};

use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use symphonia::core::{
    audio::{AudioBufferRef, Signal},
    codecs::DecoderOptions,
    errors::Error as SymphoniaError,
    formats::FormatOptions,
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
    let mut resampler = SincFixedIn::<f32>::new(ratio, 2.0, params, chunk, 1)
        .map_err(|e| Error::Transcribe(format!("resampler: {e}")))?;

    let mut out: Vec<f32> = Vec::with_capacity((input.len() as f64 * ratio) as usize + chunk);
    let mut pos = 0;
    while pos + chunk <= input.len() {
        let frame = &input[pos..pos + chunk];
        let processed = resampler
            .process(&[frame], None)
            .map_err(|e| Error::Transcribe(format!("resample: {e}")))?;
        out.extend_from_slice(&processed[0]);
        pos += chunk;
    }
    if pos < input.len() {
        let mut tail = input[pos..].to_vec();
        tail.resize(chunk, 0.0);
        let processed = resampler
            .process(&[tail], None)
            .map_err(|e| Error::Transcribe(format!("resample tail: {e}")))?;
        let keep = ((input.len() - pos) as f64 * ratio) as usize;
        out.extend_from_slice(&processed[0][..keep.min(processed[0].len())]);
    }
    Ok(out)
}
