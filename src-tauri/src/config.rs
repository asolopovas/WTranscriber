use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use crate::{
    error::Result,
    models::{Family, by_id, default_id},
    paths,
};

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
    #[serde(default)]
    pub diarizer: DiarizerChoice,
    pub auto_rename: bool,
    #[serde(default)]
    pub llm_model: Option<String>,
    #[serde(default)]
    pub last_dir: Option<std::path::PathBuf>,
    #[serde(default)]
    pub use_persistent_models: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
#[value(rename_all = "kebab-case")]
pub enum Engine {
    #[default]
    WhisperOnnx,
    Zipformer,
    Parakeet,
    Canary,
    NemoCtc,
}

impl Engine {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WhisperOnnx => "whisper-onnx",
            Self::Zipformer => "zipformer",
            Self::Parakeet => "parakeet",
            Self::Canary => "canary",
            Self::NemoCtc => "nemo-ctc",
        }
    }
}

impl std::str::FromStr for Engine {
    type Err = ();
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "whisper-onnx" => Ok(Self::WhisperOnnx),
            "zipformer" => Ok(Self::Zipformer),
            "parakeet" => Ok(Self::Parakeet),
            "canary" => Ok(Self::Canary),
            "nemo-ctc" => Ok(Self::NemoCtc),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
#[value(rename_all = "lowercase")]
pub enum Device {
    Cpu,
    Cuda,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiarizerChoice {
    #[default]
    Auto,
    Nemo,
    #[serde(alias = "sherpa")]
    Eres2net,
    Titanet,
}

impl DiarizerChoice {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Nemo => "nemo-sortformer-v2",
            Self::Eres2net => "diar-eres2net-base",
            Self::Titanet => "sherpa-pyannote-titanet",
        }
    }

    #[must_use]
    pub const fn embedding_rel(self) -> &'static str {
        match self {
            Self::Titanet => "titanet_large.onnx",
            _ => "3dspeaker_eres2net_base.onnx",
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        let (model, engine) = default_asr();
        Self {
            model,
            engine,
            language: "en".into(),
            device: default_device(),
            threads: num_threads(),
            diarize: true,
            speakers: None,
            diarizer: DiarizerChoice::default(),
            auto_rename: false,
            llm_model: None,
            last_dir: None,
            use_persistent_models: false,
        }
    }
}

fn default_asr() -> (String, Engine) {
    if let Some(id) = default_id(Family::Asr)
        && let Some(entry) = by_id(id)
    {
        return (
            id.to_string(),
            entry.engine_kind().unwrap_or(Engine::WhisperOnnx),
        );
    }
    ("sherpa-whisper-turbo".into(), Engine::WhisperOnnx)
}

fn migrate_for_platform(cfg: &mut Config) -> bool {
    if !cfg!(target_os = "android") {
        return false;
    }
    let mut dirty = false;
    if let Some(p) = cfg.last_dir.as_ref() {
        let s = p.to_string_lossy();
        if s.starts_with("/sdcard/Documents/WTranscriber")
            || s.starts_with("/storage/emulated/0/Documents/WTranscriber")
        {
            cfg.last_dir = None;
            dirty = true;
        }
    }
    if let Some(saved) = by_id(&cfg.model)
        && !saved.android_default
        && let Some(target_id) = default_id(Family::Asr)
        && target_id != cfg.model
        && let Some(target) = by_id(target_id)
    {
        let installed =
            crate::models::paths_for(target).is_ok_and(|paths| paths.iter().all(|p| p.exists()));
        if installed {
            cfg.model = target_id.to_string();
            if let Some(engine) = target.engine_kind() {
                cfg.engine = engine;
            }
            dirty = true;
        }
    }
    dirty
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
        let mut cfg: Self = serde_json::from_str(&raw).unwrap_or_default();
        if migrate_for_platform(&mut cfg) {
            let _ = cfg.save();
        }
        Ok(cfg)
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
    #[cfg(not(target_os = "android"))]
    fn default_asr_desktop_is_whisper_turbo() {
        let cfg = Config::default();
        assert_eq!(cfg.model, "sherpa-whisper-turbo");
        assert!(matches!(cfg.engine, Engine::WhisperOnnx));
    }

    #[test]
    #[cfg(target_os = "android")]
    fn default_asr_android_is_parakeet_v3() {
        let cfg = Config::default();
        assert_eq!(cfg.model, "parakeet-tdt-0.6b-v3-int8");
        assert!(matches!(cfg.engine, Engine::Parakeet));
    }

    #[test]
    fn engine_string_matches_catalog_values() {
        assert_eq!(Engine::WhisperOnnx.as_str(), "whisper-onnx");
        assert_eq!(Engine::Zipformer.as_str(), "zipformer");
        assert_eq!(Engine::Parakeet.as_str(), "parakeet");
        assert_eq!(Engine::Canary.as_str(), "canary");
        assert_eq!(Engine::NemoCtc.as_str(), "nemo-ctc");
    }

    #[test]
    fn engine_from_str_roundtrip() {
        for e in [
            Engine::WhisperOnnx,
            Engine::Zipformer,
            Engine::Parakeet,
            Engine::Canary,
            Engine::NemoCtc,
        ] {
            assert_eq!(e.as_str().parse::<Engine>().unwrap(), e);
        }
    }

    #[test]
    fn engine_from_str_rejects_unknown() {
        assert!("nonsense".parse::<Engine>().is_err());
    }

    #[test]
    fn diarizer_choice_maps_to_catalog_ids() {
        assert_eq!(DiarizerChoice::Nemo.as_str(), "nemo-sortformer-v2");
        assert_eq!(DiarizerChoice::Eres2net.as_str(), "diar-eres2net-base");
        assert_eq!(DiarizerChoice::Titanet.as_str(), "sherpa-pyannote-titanet");
        assert_eq!(DiarizerChoice::Auto.as_str(), "auto");
    }

    #[test]
    fn diarizer_titanet_uses_titanet_embedding() {
        assert_eq!(
            DiarizerChoice::Titanet.embedding_rel(),
            "titanet_large.onnx"
        );
        assert_eq!(
            DiarizerChoice::Eres2net.embedding_rel(),
            "3dspeaker_eres2net_base.onnx"
        );
    }

    #[test]
    fn diarizer_choice_deserialises_sherpa_alias_to_eres2net() {
        let v: DiarizerChoice = serde_json::from_str("\"sherpa\"").unwrap();
        assert!(matches!(v, DiarizerChoice::Eres2net));
    }
}
