use serde::{Deserialize, Serialize};

use crate::{error::Result, paths};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub model: String,
    pub language: String,
    pub device: Device,
    pub threads: u32,
    pub diarize: bool,
    pub speakers: Option<u32>,
    pub auto_rename: bool,
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
            language: "en".into(),
            device: Device::Cpu,
            threads: num_threads(),
            diarize: true,
            speakers: None,
            auto_rename: false,
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
    u32::try_from(
        std::thread::available_parallelism()
            .map_or(4, std::num::NonZero::get),
    )
    .unwrap_or(4)
}
