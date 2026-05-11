use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use crate::{
    error::{Error, Result},
    logfile,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiarizerChoice {
    #[serde(alias = "auto", alias = "sortformer")]
    SortformerOnnx,
    #[serde(alias = "nemo-python")]
    Nemo,
    #[serde(alias = "sherpa", alias = "eres2net")]
    Titanet,
}

impl Default for DiarizerChoice {
    fn default() -> Self {
        if cfg!(any(target_os = "android", target_os = "ios")) {
            // Mobile: pure-ONNX Titanet (~22 MB, CPU-friendly).
            Self::Titanet
        } else {
            // Desktop: Sortformer v2.1 ONNX (~492 MB, NeMo-grade quality, no
            // Python runtime, GPU-accelerated when available).
            Self::SortformerOnnx
        }
    }
}

impl DiarizerChoice {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SortformerOnnx => "sortformer-v2-onnx-4spk",
            Self::Nemo => "nemo-sortformer-v2",
            Self::Titanet => "sherpa-pyannote-titanet",
        }
    }

    #[must_use]
    pub const fn embedding_rel(self) -> &'static str {
        match self {
            Self::SortformerOnnx | Self::Nemo | Self::Titanet => "titanet_large.onnx",
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
            llm_model: default_llm_model(),
            last_dir: None,
            use_persistent_models: cfg!(target_os = "android"),
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

fn default_llm_model() -> Option<String> {
    default_id(Family::Llm).map(String::from)
}

fn migrate_for_platform(cfg: &mut Config) -> bool {
    let mut dirty = false;
    if cfg.llm_model.is_none() {
        let next = default_llm_model();
        if next.is_some() {
            cfg.llm_model = next;
            dirty = true;
        }
    }
    if !cfg!(target_os = "android") {
        return dirty;
    }
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

fn corrupt_config_backup_path(path: &std::path::Path) -> std::path::PathBuf {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let name = path.file_name().and_then(|n| n.to_str()).map_or_else(
        || format!("config.corrupt-{stamp}.bak"),
        |name| format!("{name}.corrupt-{stamp}.bak"),
    );
    path.with_file_name(name)
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
        let mut cfg: Self = match serde_json::from_str(&raw) {
            Ok(cfg) => cfg,
            Err(e) => {
                let backup = corrupt_config_backup_path(&path);
                logfile::error(&format!(
                    "config parse failed ({}): {e}; backing up to {}",
                    path.display(),
                    backup.display()
                ));
                std::fs::copy(&path, &backup)?;
                return Err(Error::Config(format!(
                    "invalid config file {}; backed up to {}: {e}",
                    path.display(),
                    backup.display()
                )));
            }
        };
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
    } else if cfg!(feature = "cuda") {
        Device::Cuda
    } else {
        Device::Cpu
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
        assert_eq!(
            DiarizerChoice::SortformerOnnx.as_str(),
            "sortformer-v2-onnx-4spk"
        );
        assert_eq!(DiarizerChoice::Nemo.as_str(), "nemo-sortformer-v2");
        assert_eq!(DiarizerChoice::Titanet.as_str(), "sherpa-pyannote-titanet");
    }

    #[test]
    fn diarizer_titanet_uses_titanet_embedding() {
        assert_eq!(
            DiarizerChoice::Titanet.embedding_rel(),
            "titanet_large.onnx"
        );
    }

    #[test]
    fn invalid_config_is_backed_up_and_reported() {
        let _g = crate::paths::PATHS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        crate::paths::clear_test_overrides();
        let tmp = tempfile::tempdir().unwrap();
        let config_file = tmp.path().join("config.yml");
        crate::paths::set_config_file(config_file.clone());
        std::fs::write(&config_file, "not-json").unwrap();

        let err = Config::load().unwrap_err();

        assert!(err.to_string().contains("invalid config file"));
        assert_eq!(std::fs::read_to_string(&config_file).unwrap(), "not-json");
        let backups = std::fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(std::result::Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("config.yml.corrupt-")
            })
            .count();
        assert_eq!(backups, 1);
        crate::paths::clear_test_overrides();
    }

    #[test]
    fn diarizer_choice_deserialises_legacy_aliases_to_titanet() {
        let v: DiarizerChoice = serde_json::from_str("\"sherpa\"").unwrap();
        assert!(matches!(v, DiarizerChoice::Titanet));
        let v: DiarizerChoice = serde_json::from_str("\"eres2net\"").unwrap();
        assert!(matches!(v, DiarizerChoice::Titanet));
    }

    #[test]
    fn diarizer_choice_deserialises_auto_alias_to_sortformer_onnx() {
        let v: DiarizerChoice = serde_json::from_str("\"auto\"").unwrap();
        assert!(matches!(v, DiarizerChoice::SortformerOnnx));
        let v: DiarizerChoice = serde_json::from_str("\"sortformer\"").unwrap();
        assert!(matches!(v, DiarizerChoice::SortformerOnnx));
    }

    #[test]
    fn default_config_has_concrete_llm_model() {
        let cfg = Config::default();
        assert!(
            cfg.llm_model.is_some(),
            "llm_model must always default to a concrete id"
        );
    }
}
