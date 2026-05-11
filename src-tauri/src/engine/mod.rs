mod canary;
mod chunk;
mod nemo_ctc;
mod processor;
mod recognizer;
mod runtime;
mod sherpa;
mod transducer;
mod whisper;

pub use runtime::threads;

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
    // CUDA path: in-process only when this build linked against the GPU
    // sherpa-onnx shared libs (cargo feature `cuda`). Otherwise the in-process
    // FFI would silently run on CPU, so prefer the subprocess which has its
    // own RPATH to the GPU runtime. The runtime CUDA->CPU fallback inside
    // `recognizer::build()` still protects us if the GPU EP fails to load.
    if matches!(config.device, crate::config::Device::Cuda) && !cfg!(feature = "cuda") {
        return false;
    }
    // Explicit opt-out for debugging or when the user wants subprocess on CUDA.
    if matches!(config.device, crate::config::Device::Cuda)
        && std::env::var("WT_NO_INPROCESS_CUDA")
            .ok()
            .is_some_and(|v| v == "1")
    {
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

#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use super::*;
    use crate::config::{Config, Device, Engine};
    use std::sync::Mutex;

    // Tests in this module mutate process-wide env vars; serialise them so
    // parallel test threads cannot observe each other's writes.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn cfg(engine: Engine, device: Device) -> Config {
        Config {
            engine,
            device,
            ..Config::default()
        }
    }

    fn set_env(key: &str, value: &str) {
        // SAFETY: callers hold ENV_LOCK; no other thread reads/writes these
        // vars concurrently inside this test module.
        unsafe {
            std::env::set_var(key, value);
        }
    }

    fn clear_env() {
        for k in [
            "WT_USE_SUBPROCESS",
            "WT_FORCE_INPROCESS",
            "WT_NO_INPROCESS_CUDA",
        ] {
            // SAFETY: see `set_env`.
            unsafe {
                std::env::remove_var(k);
            }
        }
    }

    #[test]
    fn use_in_process_decision_matrix() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_env();

        // Canary has no in-process path (recognizer rejects it).
        assert!(!use_in_process(&cfg(Engine::Canary, Device::Cpu)));
        assert!(!use_in_process(&cfg(Engine::Canary, Device::Cuda)));

        // CPU-side inprocess engines route to inprocess by default.
        for e in [
            Engine::WhisperOnnx,
            Engine::Zipformer,
            Engine::Parakeet,
            Engine::NemoCtc,
        ] {
            assert!(
                use_in_process(&cfg(e, Device::Cpu)),
                "{e:?} on CPU should be in-process"
            );
        }

        // CUDA on a non-CUDA build must take the subprocess path.
        if !cfg!(feature = "cuda") {
            assert!(!use_in_process(&cfg(Engine::WhisperOnnx, Device::Cuda)));
            assert!(!use_in_process(&cfg(Engine::Parakeet, Device::Cuda)));
        }
    }

    #[test]
    fn use_in_process_env_overrides() {
        let _g = ENV_LOCK.lock().unwrap();

        // WT_USE_SUBPROCESS forces subprocess everywhere.
        clear_env();
        set_env("WT_USE_SUBPROCESS", "1");
        assert!(!use_in_process(&cfg(Engine::WhisperOnnx, Device::Cpu)));
        assert!(!use_in_process(&cfg(Engine::Parakeet, Device::Cpu)));

        // WT_FORCE_INPROCESS overrides the cuda-feature gate.
        clear_env();
        set_env("WT_FORCE_INPROCESS", "1");
        assert!(use_in_process(&cfg(Engine::WhisperOnnx, Device::Cuda)));
        assert!(use_in_process(&cfg(Engine::NemoCtc, Device::Cuda)));
        // Canary still subprocess-only even under force.
        assert!(!use_in_process(&cfg(Engine::Canary, Device::Cuda)));

        // WT_NO_INPROCESS_CUDA only affects CUDA builds.
        if cfg!(feature = "cuda") {
            clear_env();
            set_env("WT_NO_INPROCESS_CUDA", "1");
            assert!(!use_in_process(&cfg(Engine::WhisperOnnx, Device::Cuda)));
            // CPU path unaffected.
            assert!(use_in_process(&cfg(Engine::WhisperOnnx, Device::Cpu)));
        }

        clear_env();
    }
}
