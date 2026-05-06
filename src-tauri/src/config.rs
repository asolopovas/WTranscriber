use serde::{Deserialize, Serialize};

use crate::{error::Result, paths};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub model: String,
    pub engine: Engine,
    pub language: String,
    pub device: Device,
    pub threads: u32,
    pub diarize: bool,
    pub speakers: Option<u32>,
    pub auto_rename: bool,
    #[serde(default)]
    pub last_dir: Option<std::path::PathBuf>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Engine {
    #[default]
    WhisperOnnx,
    Zipformer,
    Parakeet,
    Canary,
    NemoCtc,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Device {
    Cpu,
    Cuda,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: "sherpa-whisper-turbo".into(),
            engine: Engine::WhisperOnnx,
            language: "en".into(),
            device: default_device(),
            threads: num_threads(),
            diarize: true,
            speakers: None,
            auto_rename: false,
            last_dir: None,
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = paths::config_file()?;
        if !path.exists() {
            let cfg = Self::default();
            cfg.save()?;
            return Ok(cfg);
        }
        let raw = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&raw).unwrap_or_default())
    }

    pub fn save(&self) -> Result<()> {
        let path = paths::config_file()?;
        let raw = serde_json::to_string_pretty(self)?;
        std::fs::write(path, raw)?;
        Ok(())
    }
}

fn num_threads() -> u32 {
    u32::try_from(std::thread::available_parallelism().map_or(4, std::num::NonZero::get))
        .unwrap_or(4)
}

const fn default_device() -> Device {
    if cfg!(any(target_os = "android", target_os = "ios")) {
        Device::Cpu
    } else {
        Device::Cuda
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_defaults_to_cuda() {
        if cfg!(not(any(target_os = "android", target_os = "ios"))) {
            assert!(matches!(Config::default().device, Device::Cuda));
        }
    }

    #[test]
    fn default_asr_is_best_quality_whisper_turbo() {
        let cfg = Config::default();
        assert_eq!(cfg.model, "sherpa-whisper-turbo");
        assert!(matches!(cfg.engine, Engine::WhisperOnnx));
    }
}
