use std::{
    path::PathBuf,
    sync::{
        Mutex, MutexGuard, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

use sherpa_onnx::{
    OfflineModelConfig, OfflineNemoEncDecCtcModelConfig, OfflineQwen3ASRModelConfig,
    OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig,
};

use crate::{
    config::{Config, Device, Engine},
    error::{Error, Result},
    logfile,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub model: String,
    pub engine: Engine,
    pub device: Device,
    pub language: String,
    pub threads: u32,
}

pub struct Loaded {
    pub key: CacheKey,
    pub recognizer: OfflineRecognizer,
}

static CACHE: OnceLock<Mutex<Option<Loaded>>> = OnceLock::new();
static CUDA_DISABLED: AtomicBool = AtomicBool::new(false);

fn cache() -> &'static Mutex<Option<Loaded>> {
    CACHE.get_or_init(|| Mutex::new(None))
}

pub fn lock() -> MutexGuard<'static, Option<Loaded>> {
    cache().lock().expect("recognizer cache poisoned")
}

pub fn ensure(config: &Config) -> Result<()> {
    let want = key_for(config);
    let mut guard = lock();
    if guard.as_ref().is_some_and(|l| l.key == want) {
        return Ok(());
    }
    *guard = None;
    drop(guard);
    let recognizer = build(config)?;
    *lock() = Some(Loaded {
        key: want,
        recognizer,
    });
    Ok(())
}

pub fn key_for(config: &Config) -> CacheKey {
    CacheKey {
        model: config.model.clone(),
        engine: config.engine,
        device: config.device,
        language: config.language.clone(),
        threads: config.threads,
    }
}

fn provider_for(config: &Config) -> &'static str {
    if CUDA_DISABLED.load(Ordering::Relaxed) {
        return "cpu";
    }
    super::runtime::provider(config).as_arg()
}

fn build_config(config: &Config, provider: &str, threads: u32) -> Result<OfflineRecognizerConfig> {
    match config.engine {
        Engine::Parakeet => transducer_config(config, provider, threads),
        Engine::NemoCtc => nemo_ctc_config(config, provider, threads),
        Engine::Qwen3Asr => qwen3_asr_config(config, provider, threads),
        Engine::WhisperCpp => Err(Error::Config(
            "whisper-cpp engine bypasses the sherpa recognizer; \
             dispatch happens in engine::run"
                .into(),
        )),
    }
}

fn build(config: &Config) -> Result<OfflineRecognizer> {
    let t0 = std::time::Instant::now();
    let provider = provider_for(config);
    let threads = crate::engine::threads(config);
    logfile::info(&format!(
        "engine init: model={} engine={} device={} threads={}",
        config.model,
        config.engine.as_str(),
        provider,
        threads,
    ));
    let cfg = build_config(config, provider, threads)?;
    if let Some(rec) = OfflineRecognizer::create(&cfg) {
        logfile::info(&format!(
            "engine ready in {:.2}s",
            t0.elapsed().as_secs_f64()
        ));
        return Ok(rec);
    }
    if provider == "cuda" && !CUDA_DISABLED.swap(true, Ordering::Relaxed) {
        logfile::warn(
            "OfflineRecognizer::create failed with provider=cuda; falling back to CPU. \
             Verify CUDA 12.x runtime, cuDNN 9, and the prebuilt sherpa-onnx CUDA archive \
             (run `just sherpa-cuda` and `just cudnn`).",
        );
        let cfg = build_config(config, "cpu", threads)?;
        if let Some(rec) = OfflineRecognizer::create(&cfg) {
            logfile::info(&format!(
                "engine ready (cpu fallback) in {:.2}s",
                t0.elapsed().as_secs_f64()
            ));
            return Ok(rec);
        }
    }
    Err(Error::Transcribe(
        "OfflineRecognizer::create returned None".into(),
    ))
}

fn model_dir(model_id: &str) -> Result<PathBuf> {
    crate::models::model_dir(model_id)
}

fn locate_three(
    dir: &std::path::Path,
    model_id: &str,
    suffixes: &[&str; 3],
) -> Result<[PathBuf; 3]> {
    let no_sherpa = model_id.strip_prefix("sherpa-").unwrap_or(model_id);
    let last_segment = model_id.rsplit('-').next().unwrap_or(model_id);
    let stems: &[&str] = &[model_id, no_sherpa, last_segment, ""];
    let int8_suffixes: [String; 3] = [
        suffixes[0].replace(".onnx", ".int8.onnx"),
        suffixes[1].replace(".onnx", ".int8.onnx"),
        suffixes[2].replace(".onnx", ".int8.onnx"),
    ];
    let variants: [[&str; 3]; 2] = [
        [&int8_suffixes[0], &int8_suffixes[1], &int8_suffixes[2]],
        [suffixes[0], suffixes[1], suffixes[2]],
    ];
    for variant in &variants {
        for stem in stems {
            let prefix = if stem.is_empty() {
                String::new()
            } else {
                format!("{stem}-")
            };
            let p0 = dir.join(format!("{prefix}{}", variant[0]));
            let p1 = dir.join(format!("{prefix}{}", variant[1]));
            let p2 = dir.join(format!("{prefix}{}", variant[2]));
            if p0.exists() && p1.exists() && p2.exists() {
                return Ok([p0, p1, p2]);
            }
        }
    }
    Err(Error::Transcribe(format!(
        "model files {:?} missing in {}",
        suffixes,
        dir.display()
    )))
}

fn transducer_config(
    config: &Config,
    provider: &str,
    threads: u32,
) -> Result<OfflineRecognizerConfig> {
    let dir = model_dir(&config.model)?;
    let [encoder, decoder, joiner] = locate_three(
        &dir,
        &config.model,
        &["encoder.onnx", "decoder.onnx", "joiner.onnx"],
    )?;
    let tokens = dir.join("tokens.txt");
    if !tokens.exists() {
        return Err(Error::Transcribe(format!(
            "tokens.txt missing in {}",
            dir.display()
        )));
    }
    Ok(OfflineRecognizerConfig {
        model_config: OfflineModelConfig {
            transducer: OfflineTransducerModelConfig {
                encoder: Some(encoder.to_string_lossy().into_owned()),
                decoder: Some(decoder.to_string_lossy().into_owned()),
                joiner: Some(joiner.to_string_lossy().into_owned()),
            },
            tokens: Some(tokens.to_string_lossy().into_owned()),
            provider: Some(provider.into()),
            num_threads: i32::try_from(threads.max(1)).unwrap_or(1),
            debug: true,
            ..OfflineModelConfig::default()
        },
        ..OfflineRecognizerConfig::default()
    })
}

fn nemo_ctc_config(
    config: &Config,
    provider: &str,
    threads: u32,
) -> Result<OfflineRecognizerConfig> {
    let dir = model_dir(&config.model)?;
    let tokens = dir.join("tokens.txt");
    let model = ["model.int8.onnx", "model.onnx"]
        .into_iter()
        .map(|n| dir.join(n))
        .find(|p| p.exists())
        .ok_or_else(|| {
            Error::Transcribe(format!(
                "model(.int8).onnx or tokens.txt missing in {}",
                dir.display()
            ))
        })?;
    if !tokens.exists() {
        return Err(Error::Transcribe(format!(
            "tokens.txt missing in {}",
            dir.display()
        )));
    }
    Ok(OfflineRecognizerConfig {
        model_config: OfflineModelConfig {
            nemo_ctc: OfflineNemoEncDecCtcModelConfig {
                model: Some(model.to_string_lossy().into_owned()),
            },
            tokens: Some(tokens.to_string_lossy().into_owned()),
            provider: Some(provider.into()),
            num_threads: i32::try_from(threads.max(1)).unwrap_or(1),
            debug: true,
            ..OfflineModelConfig::default()
        },
        ..OfflineRecognizerConfig::default()
    })
}

fn qwen3_asr_config(
    config: &Config,
    provider: &str,
    threads: u32,
) -> Result<OfflineRecognizerConfig> {
    let dir = model_dir(&config.model)?;
    let conv_frontend = dir.join("conv_frontend.onnx");
    let encoder = ["encoder.int8.onnx", "encoder.onnx"]
        .into_iter()
        .map(|n| dir.join(n))
        .find(|p| p.exists());
    let decoder = ["decoder.int8.onnx", "decoder.onnx"]
        .into_iter()
        .map(|n| dir.join(n))
        .find(|p| p.exists());
    let tokenizer = dir.join("tokenizer");
    let (Some(encoder), Some(decoder)) = (encoder, decoder) else {
        return Err(Error::Transcribe(format!(
            "encoder/decoder(.int8).onnx missing in {}",
            dir.display()
        )));
    };
    if !conv_frontend.exists() || !tokenizer.is_dir() {
        return Err(Error::Transcribe(format!(
            "conv_frontend.onnx or tokenizer/ missing in {}",
            dir.display()
        )));
    }
    Ok(OfflineRecognizerConfig {
        model_config: OfflineModelConfig {
            qwen3_asr: OfflineQwen3ASRModelConfig {
                conv_frontend: Some(conv_frontend.to_string_lossy().into_owned()),
                encoder: Some(encoder.to_string_lossy().into_owned()),
                decoder: Some(decoder.to_string_lossy().into_owned()),
                tokenizer: Some(tokenizer.to_string_lossy().into_owned()),
                ..OfflineQwen3ASRModelConfig::default()
            },
            provider: Some(provider.into()),
            num_threads: i32::try_from(threads.max(1)).unwrap_or(1),
            debug: true,
            ..OfflineModelConfig::default()
        },
        ..OfflineRecognizerConfig::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, Device, Engine};

    fn cfg(model: &str, engine: Engine, device: Device, threads: u32, lang: &str) -> Config {
        Config {
            model: model.into(),
            engine,
            device,
            threads,
            language: lang.into(),
            ..Config::default()
        }
    }

    #[test]
    fn key_equal_when_relevant_fields_match() {
        let a = key_for(&cfg("m", Engine::Parakeet, Device::Cpu, 4, "en"));
        let b = key_for(&cfg("m", Engine::Parakeet, Device::Cpu, 4, "en"));
        assert_eq!(a, b);
    }

    #[test]
    fn key_differs_when_model_differs() {
        let a = key_for(&cfg("m1", Engine::Parakeet, Device::Cpu, 4, "en"));
        let b = key_for(&cfg("m2", Engine::Parakeet, Device::Cpu, 4, "en"));
        assert_ne!(a, b);
    }

    #[test]
    fn key_differs_when_engine_differs() {
        let a = key_for(&cfg("m", Engine::Parakeet, Device::Cpu, 4, "en"));
        let b = key_for(&cfg("m", Engine::NemoCtc, Device::Cpu, 4, "en"));
        assert_ne!(a, b);
    }

    #[test]
    fn key_differs_when_device_differs() {
        let a = key_for(&cfg("m", Engine::Parakeet, Device::Cpu, 4, "en"));
        let b = key_for(&cfg("m", Engine::Parakeet, Device::Cuda, 4, "en"));
        assert_ne!(a, b);
    }

    #[test]
    fn key_differs_when_threads_differ() {
        let a = key_for(&cfg("m", Engine::Parakeet, Device::Cpu, 4, "en"));
        let b = key_for(&cfg("m", Engine::Parakeet, Device::Cpu, 8, "en"));
        assert_ne!(a, b);
    }

    #[test]
    fn key_differs_when_language_differs() {
        let a = key_for(&cfg("m", Engine::Parakeet, Device::Cpu, 4, "en"));
        let b = key_for(&cfg("m", Engine::Parakeet, Device::Cpu, 4, "ru"));
        assert_ne!(a, b);
    }
}
