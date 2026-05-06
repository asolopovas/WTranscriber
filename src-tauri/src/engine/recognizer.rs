use std::{
    path::PathBuf,
    sync::{Mutex, MutexGuard, OnceLock},
};

use sherpa_onnx::{
    OfflineModelConfig, OfflineNemoEncDecCtcModelConfig, OfflineRecognizer,
    OfflineRecognizerConfig, OfflineTransducerModelConfig, OfflineWhisperModelConfig,
};

use crate::{
    config::{Config, Device, Engine},
    error::{Error, Result},
    logfile, paths,
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

fn cache() -> &'static Mutex<Option<Loaded>> {
    CACHE.get_or_init(|| Mutex::new(None))
}

pub fn lock() -> MutexGuard<'static, Option<Loaded>> {
    cache().lock().expect("recognizer cache poisoned")
}

pub fn invalidate() {
    *lock() = None;
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

fn provider(device: Device) -> &'static str {
    match device {
        Device::Cuda => "cuda",
        Device::Cpu => "cpu",
    }
}

fn build(config: &Config) -> Result<OfflineRecognizer> {
    let t0 = std::time::Instant::now();
    let cfg = match config.engine {
        Engine::WhisperOnnx => whisper_config(config)?,
        Engine::Zipformer => transducer_config(config, "zipformer")?,
        Engine::Parakeet => transducer_config(config, "parakeet")?,
        Engine::NemoCtc => nemo_ctc_config(config)?,
        Engine::Canary => {
            return Err(Error::Config(
                "canary engine in-process path not yet implemented".into(),
            ));
        }
    };
    logfile::info(&format!(
        "engine init: model={} engine={} device={} threads={}",
        config.model,
        config.engine.as_str(),
        provider(config.device),
        config.threads,
    ));
    let recognizer = OfflineRecognizer::create(&cfg)
        .ok_or_else(|| Error::Transcribe("OfflineRecognizer::create returned None".into()))?;
    logfile::info(&format!(
        "engine ready in {:.2}s",
        t0.elapsed().as_secs_f64()
    ));
    Ok(recognizer)
}

fn model_dir(model_id: &str) -> Result<PathBuf> {
    Ok(paths::models_dir()?.join(model_id))
}

fn locate_three(
    dir: &std::path::Path,
    model_id: &str,
    suffixes: &[&str; 3],
) -> Result<[PathBuf; 3]> {
    let stems: &[&str] = &[
        model_id,
        model_id.strip_prefix("sherpa-").unwrap_or(model_id),
        "",
    ];
    for stem in stems {
        let prefix = if stem.is_empty() {
            String::new()
        } else {
            format!("{stem}-")
        };
        let p0 = dir.join(format!("{prefix}{}", suffixes[0]));
        let p1 = dir.join(format!("{prefix}{}", suffixes[1]));
        let p2 = dir.join(format!("{prefix}{}", suffixes[2]));
        if p0.exists() && p1.exists() && p2.exists() {
            return Ok([p0, p1, p2]);
        }
    }
    Err(Error::Transcribe(format!(
        "model files {:?} missing in {}",
        suffixes,
        dir.display()
    )))
}

fn whisper_config(config: &Config) -> Result<OfflineRecognizerConfig> {
    let dir = model_dir(&config.model)?;
    let [encoder, decoder, tokens] =
        locate_three(&dir, &config.model, &["encoder.int8.onnx", "decoder.int8.onnx", "tokens.txt"])?;
    let language = (config.language != "auto" && !config.language.is_empty())
        .then(|| config.language.clone());
    let mut rc = OfflineRecognizerConfig::default();
    rc.model_config = OfflineModelConfig {
        whisper: OfflineWhisperModelConfig {
            encoder: Some(encoder.to_string_lossy().into_owned()),
            decoder: Some(decoder.to_string_lossy().into_owned()),
            language,
            task: Some("transcribe".into()),
            tail_paddings: -1,
            enable_token_timestamps: true,
            enable_segment_timestamps: false,
        },
        tokens: Some(tokens.to_string_lossy().into_owned()),
        provider: Some(provider(config.device).into()),
        num_threads: i32::try_from(config.threads.max(1)).unwrap_or(1),
        debug: false,
        ..OfflineModelConfig::default()
    };
    Ok(rc)
}

fn transducer_config(config: &Config, _kind: &str) -> Result<OfflineRecognizerConfig> {
    let dir = model_dir(&config.model)?;
    let [encoder, decoder, joiner] =
        locate_three(&dir, &config.model, &["encoder.onnx", "decoder.onnx", "joiner.onnx"])?;
    let tokens = dir.join("tokens.txt");
    if !tokens.exists() {
        return Err(Error::Transcribe(format!(
            "tokens.txt missing in {}",
            dir.display()
        )));
    }
    let mut rc = OfflineRecognizerConfig::default();
    rc.model_config = OfflineModelConfig {
        transducer: OfflineTransducerModelConfig {
            encoder: Some(encoder.to_string_lossy().into_owned()),
            decoder: Some(decoder.to_string_lossy().into_owned()),
            joiner: Some(joiner.to_string_lossy().into_owned()),
        },
        tokens: Some(tokens.to_string_lossy().into_owned()),
        provider: Some(provider(config.device).into()),
        num_threads: i32::try_from(config.threads.max(1)).unwrap_or(1),
        debug: false,
        ..OfflineModelConfig::default()
    };
    Ok(rc)
}

fn nemo_ctc_config(config: &Config) -> Result<OfflineRecognizerConfig> {
    let dir = model_dir(&config.model)?;
    let model = dir.join("model.onnx");
    let tokens = dir.join("tokens.txt");
    if !model.exists() || !tokens.exists() {
        return Err(Error::Transcribe(format!(
            "model.onnx or tokens.txt missing in {}",
            dir.display()
        )));
    }
    let mut rc = OfflineRecognizerConfig::default();
    rc.model_config = OfflineModelConfig {
        nemo_ctc: OfflineNemoEncDecCtcModelConfig {
            model: Some(model.to_string_lossy().into_owned()),
        },
        tokens: Some(tokens.to_string_lossy().into_owned()),
        provider: Some(provider(config.device).into()),
        num_threads: i32::try_from(config.threads.max(1)).unwrap_or(1),
        debug: false,
        ..OfflineModelConfig::default()
    };
    Ok(rc)
}
