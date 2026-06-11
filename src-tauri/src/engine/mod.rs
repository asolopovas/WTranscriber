mod chunk;
mod nemo_ctc;
mod processor;
mod recognizer;
mod runtime;
mod sherpa;
mod transducer;
#[cfg(not(target_os = "ios"))]
mod whisper_cpp;

pub use runtime::threads;

use crate::{
    config::{Config, Engine},
    error::{Error, Result},
    transcriber::Segment,
};

pub fn shutdown() {
    #[cfg(not(target_os = "ios"))]
    whisper_cpp::shutdown_worker();
}

pub fn resolve_device(config: &mut Config) -> Option<String> {
    if !matches!(config.device, crate::config::Device::Cuda) || cfg!(feature = "cuda") {
        return None;
    }
    match config.engine {
        Engine::WhisperCpp => {
            #[cfg(not(target_os = "ios"))]
            if whisper_cpp::cuda_worker_available() {
                return None;
            }
            config.device = crate::config::Device::Cpu;
            Some(
                "CUDA requested but no Whisper CUDA worker is installed; transcribing on CPU"
                    .into(),
            )
        }
        Engine::Parakeet | Engine::NemoCtc => {
            if crate::runtimes::dependencies::onnx_provider(config.device) == "cuda" {
                None
            } else {
                Some(
                    "CUDA requested but this build has no ONNX CUDA runtime; transcribing on CPU"
                        .into(),
                )
            }
        }
    }
}

pub fn preflight(config: &Config) -> Result<()> {
    if use_in_process(config) {
        return Ok(());
    }
    if matches!(config.engine, Engine::WhisperCpp) {
        return Ok(());
    }
    sherpa::find_binary().map(|_| ())
}

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
        Engine::Parakeet => {
            transducer::run(samples, audio_dur_sec, config, on_progress, cancelled)?
        }
        Engine::NemoCtc => nemo_ctc::run(samples, audio_dur_sec, config, on_progress, cancelled)?,
        Engine::WhisperCpp => {
            #[cfg(not(target_os = "ios"))]
            {
                whisper_cpp::run(samples, audio_dur_sec, config, on_progress, cancelled)?
            }
            #[cfg(target_os = "ios")]
            {
                return Err(Error::Config(
                    "whisper-cpp engine is unavailable on this build".into(),
                ));
            }
        }
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
        return matches!(config.engine, Engine::Parakeet | Engine::NemoCtc);
    }

    if matches!(config.device, crate::config::Device::Cuda) && !cfg!(feature = "cuda") {
        return false;
    }

    if matches!(config.device, crate::config::Device::Cuda)
        && std::env::var("WT_NO_INPROCESS_CUDA")
            .ok()
            .is_some_and(|v| v == "1")
    {
        return false;
    }
    matches!(config.engine, Engine::Parakeet | Engine::NemoCtc)
}

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
    let chunk_sec = audio_dur_sec.max(1.0);
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

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn cfg(engine: Engine, device: Device) -> Config {
        Config {
            engine,
            device,
            ..Config::default()
        }
    }

    fn set_env(key: &str, value: &str) {
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
            unsafe {
                std::env::remove_var(k);
            }
        }
    }

    #[test]
    fn use_in_process_decision_matrix() {
        let _g = ENV_LOCK.lock().unwrap();
        clear_env();

        assert!(!use_in_process(&cfg(Engine::WhisperCpp, Device::Cpu)));
        assert!(!use_in_process(&cfg(Engine::WhisperCpp, Device::Cuda)));

        for e in [Engine::Parakeet, Engine::NemoCtc] {
            assert!(
                use_in_process(&cfg(e, Device::Cpu)),
                "{e:?} on CPU should be in-process"
            );
        }

        if !cfg!(feature = "cuda") {
            assert!(!use_in_process(&cfg(Engine::Parakeet, Device::Cuda)));
        }
    }

    #[test]
    fn use_in_process_env_overrides() {
        let _g = ENV_LOCK.lock().unwrap();

        clear_env();
        set_env("WT_USE_SUBPROCESS", "1");
        assert!(!use_in_process(&cfg(Engine::Parakeet, Device::Cpu)));

        clear_env();
        set_env("WT_FORCE_INPROCESS", "1");
        assert!(use_in_process(&cfg(Engine::Parakeet, Device::Cuda)));
        assert!(use_in_process(&cfg(Engine::NemoCtc, Device::Cuda)));
        assert!(!use_in_process(&cfg(Engine::WhisperCpp, Device::Cuda)));

        if cfg!(feature = "cuda") {
            clear_env();
            set_env("WT_NO_INPROCESS_CUDA", "1");
            assert!(!use_in_process(&cfg(Engine::Parakeet, Device::Cuda)));
            assert!(use_in_process(&cfg(Engine::Parakeet, Device::Cpu)));
        }

        clear_env();
    }
}
