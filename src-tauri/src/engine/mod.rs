mod canary;
mod chunk;
mod nemo_ctc;
mod processor;
mod recognizer;
mod runtime;
mod sherpa;
mod transducer;
mod whisper;

#[allow(unused_imports)]
pub use chunk::{Chunk, run_chunked, run_single, split_chunks};
#[allow(unused_imports)]
pub use runtime::{Provider, provider, threads};
#[allow(unused_imports)]
pub use sherpa::{SherpaResult, find_binary, parse_json, run_cmd};
#[allow(unused_imports)]
pub use whisper::run as run_whisper;

use crate::{
    config::{Config, Engine},
    error::{Error, Result},
    transcriber::Segment,
};

pub fn run(
    samples: &[f32],
    audio_dur_sec: f64,
    config: &Config,
    on_progress: &mut dyn FnMut(f64),
    cancelled: &dyn Fn() -> bool,
    on_chunk: &mut dyn FnMut(Vec<Segment>, f64),
) -> Result<(String, f64)> {
    if use_in_process(config) {
        return run_in_process(samples, audio_dur_sec, config, cancelled, on_chunk);
    }
    let (segs, lang, rtf) = match config.engine {
        Engine::WhisperOnnx => {
            whisper::run(samples, audio_dur_sec, config, on_progress, cancelled)?
        }
        Engine::Zipformer => transducer::run(
            transducer::Kind::Zipformer,
            samples,
            audio_dur_sec,
            config,
            on_progress,
            cancelled,
        )?,
        Engine::Parakeet => transducer::run(
            transducer::Kind::Parakeet,
            samples,
            audio_dur_sec,
            config,
            on_progress,
            cancelled,
        )?,
        Engine::Canary => canary::run(samples, audio_dur_sec, config, on_progress, cancelled)?,
        Engine::NemoCtc => nemo_ctc::run(samples, audio_dur_sec, config, on_progress, cancelled)?,
    };
    on_chunk(segs, audio_dur_sec);
    Ok((lang, rtf))
}

fn use_in_process(config: &Config) -> bool {
    if std::env::var("WT_USE_SUBPROCESS")
        .ok()
        .is_some_and(|v| v == "1")
    {
        return false;
    }
    if std::env::var("WT_FORCE_INPROCESS")
        .ok()
        .is_some_and(|v| v == "1")
    {
        return matches!(
            config.engine,
            Engine::WhisperOnnx | Engine::Zipformer | Engine::Parakeet | Engine::NemoCtc
        );
    }
    // CUDA path: prefer the downloaded sherpa-onnx-offline subprocess (its
    // RPATH locates the GPU shared libs; the in-process FFI is statically
    // linked CPU-only on most builds, so we'd silently run on CPU).
    if matches!(config.device, crate::config::Device::Cuda) {
        return false;
    }
    matches!(
        config.engine,
        Engine::WhisperOnnx | Engine::Zipformer | Engine::Parakeet | Engine::NemoCtc
    )
}

const WHISPER_MAX_CHUNK_SEC: f64 = 15.0;

#[allow(clippy::significant_drop_tightening)]
fn run_in_process(
    samples: &[f32],
    audio_dur_sec: f64,
    config: &Config,
    cancelled: &dyn Fn() -> bool,
    on_chunk: &mut dyn FnMut(Vec<Segment>, f64),
) -> Result<(String, f64)> {
    if cancelled() {
        return Err(Error::Cancelled);
    }
    recognizer::ensure(config)?;
    let mut guard = recognizer::lock();
    let loaded = guard
        .as_mut()
        .ok_or_else(|| Error::Transcribe("recognizer cache empty after ensure".into()))?;
    let sample_rate = i32::try_from(crate::audio::WHISPER_SAMPLE_RATE).unwrap_or(16_000);
    let chunk_sec = match config.engine {
        Engine::WhisperOnnx => WHISPER_MAX_CHUNK_SEC,
        _ => audio_dur_sec.max(1.0),
    };
    let chunks = chunk::split_chunks(samples, chunk_sec);

    let t0 = std::time::Instant::now();
    for ch in &chunks {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        let stream = loaded.recognizer.create_stream();
        stream.accept_waveform(sample_rate, ch.samples);
        loaded.recognizer.decode(&stream);
        let chunk_dur = ch.end_sec - ch.start_sec;
        let result = stream
            .get_result()
            .ok_or_else(|| Error::Transcribe("empty result from recognizer".into()))?;
        let mut chunk_segs = build_segments(&result, chunk_dur);
        let offset_ms = chunk::ms(ch.start_sec);
        for seg in &mut chunk_segs {
            seg.start_ms = seg.start_ms.saturating_add(offset_ms);
            seg.end_ms = seg.end_ms.saturating_add(offset_ms);
            for tok in &mut seg.tokens {
                tok.start_ms = tok.start_ms.saturating_add(offset_ms);
                tok.end_ms = tok.end_ms.saturating_add(offset_ms);
            }
        }
        on_chunk(chunk_segs, ch.end_sec);
    }
    let elapsed = t0.elapsed().as_secs_f64();
    let rtf = if elapsed > 0.0 {
        audio_dur_sec / elapsed
    } else {
        0.0
    };
    let detected = if config.language == "auto" || config.language.is_empty() {
        String::new()
    } else {
        config.language.clone()
    };
    Ok((detected, rtf))
}

fn build_segments(
    result: &sherpa_onnx::OfflineRecognizerResult,
    audio_dur_sec: f64,
) -> Vec<Segment> {
    let stamps = result.timestamps.as_deref().unwrap_or(&[]);
    if let Some(seg) = chunk::coalesce_segment(
        &result.tokens,
        stamps.iter().copied().map(f64::from),
        audio_dur_sec,
    ) {
        return vec![seg];
    }
    let text = result.text.trim();
    if text.is_empty() {
        return Vec::new();
    }
    vec![Segment {
        text: text.to_owned(),
        start_ms: 0,
        end_ms: chunk::ms(audio_dur_sec),
        tokens: Vec::new(),
    }]
}
