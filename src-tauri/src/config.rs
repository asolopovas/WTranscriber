use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use crate::{
    constants,
    error::Result,
    logfile,
    models::{Family, by_id, default_id},
    paths,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[allow(clippy::struct_excessive_bools)]
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
    #[serde(default)]
    pub has_seen_persistent_prompt: bool,
    #[serde(default)]
    pub debug_logging: bool,
    #[serde(default)]
    pub precise_word_timestamps: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
#[value(rename_all = "kebab-case")]
pub enum Engine {
    #[default]
    Parakeet,
    NemoCtc,
    Qwen3Asr,
    WhisperCpp,
}

impl Engine {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Parakeet => "parakeet",
            Self::NemoCtc => "nemo-ctc",
            Self::Qwen3Asr => "qwen3-asr",
            Self::WhisperCpp => "whisper-cpp",
        }
    }
}

impl std::str::FromStr for Engine {
    type Err = ();
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "parakeet" => Ok(Self::Parakeet),
            "nemo-ctc" => Ok(Self::NemoCtc),
            "qwen3-asr" => Ok(Self::Qwen3Asr),
            "whisper-cpp" => Ok(Self::WhisperCpp),
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

impl Device {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Cuda => "cuda",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiarizerChoice {
    #[serde(
        alias = "auto",
        alias = "sortformer",
        alias = "nemo",
        alias = "nemo-python"
    )]
    SortformerOnnx,
    #[serde(alias = "sherpa", alias = "eres2net")]
    Titanet,
}

impl Default for DiarizerChoice {
    fn default() -> Self {
        Self::SortformerOnnx
    }
}

impl DiarizerChoice {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SortformerOnnx => "sortformer-v2-onnx-4spk",
            Self::Titanet => "sherpa-pyannote-titanet",
        }
    }

    #[must_use]
    pub const fn embedding_rel(self) -> &'static str {
        "titanet_large.onnx"
    }
}

impl Default for Config {
    fn default() -> Self {
        let (model, engine) = default_asr();
        Self {
            model,
            engine,
            language: "auto".into(),
            device: default_device(),
            threads: num_threads(),
            diarize: true,
            speakers: None,
            diarizer: DiarizerChoice::default(),
            auto_rename: false,
            llm_model: default_llm_model(),
            last_dir: None,
            use_persistent_models: cfg!(target_os = "android"),
            has_seen_persistent_prompt: false,
            debug_logging: false,
            precise_word_timestamps: false,
        }
    }
}

fn default_asr() -> (String, Engine) {
    if let Some(id) = default_id(Family::Asr)
        && let Some(entry) = by_id(id)
    {
        return (
            id.to_string(),
            entry.engine_kind().unwrap_or(Engine::Parakeet),
        );
    }
    ("parakeet-tdt-0.6b-v3-int8".into(), Engine::Parakeet)
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
    if let Some(p) = cfg.last_dir.as_ref()
        && !p.is_dir()
    {
        cfg.last_dir = None;
        dirty = true;
    }
    if !cfg!(target_os = "android") {
        return dirty;
    }
    if let Some(p) = cfg.last_dir.as_ref() {
        let s = p.to_string_lossy();
        if s.starts_with(constants::ANDROID_LEGACY_ROOT)
            || s.starts_with(constants::ANDROID_LEGACY_ROOT_EMULATED)
            || s.starts_with("/data/")
        {
            cfg.last_dir = None;
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
                logfile::warn(&format!(
                    "config parse failed ({}): {e}; backing up to {} and resetting to defaults",
                    path.display(),
                    backup.display()
                ));
                std::fs::copy(&path, &backup)?;
                let fresh = Self::default();
                fresh.save()?;
                return Ok(fresh);
            }
        };
        if migrate_for_platform(&mut cfg) {
            let _ = cfg.save();
        }
        logfile::set_debug_enabled(cfg.debug_logging);
        Ok(cfg)
    }

    pub fn save(&self) -> Result<()> {
        let path = paths::config_file()?;
        let raw = serde_json::to_string_pretty(self)?;
        std::fs::write(path, raw)?;
        logfile::set_debug_enabled(self.debug_logging);
        Ok(())
    }
}

fn num_threads() -> u32 {
    u32::try_from(
        std::thread::available_parallelism()
            .map_or(constants::DEFAULT_THREADS as usize, std::num::NonZero::get),
    )
    .unwrap_or(constants::DEFAULT_THREADS)
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
    fn desktop_defaults_to_cuda_when_cuda_feature_on() {
        if cfg!(not(any(target_os = "android", target_os = "ios"))) && cfg!(feature = "cuda") {
            assert!(matches!(Config::default().device, Device::Cuda));
        }
    }

    #[test]
    fn desktop_defaults_to_cpu_when_cuda_feature_off() {
        if cfg!(not(any(target_os = "android", target_os = "ios"))) && !cfg!(feature = "cuda") {
            assert!(matches!(Config::default().device, Device::Cpu));
        }
    }

    #[test]
    fn migrate_clears_missing_last_dir() {
        let mut cfg = Config::default();
        cfg.last_dir = Some(std::path::PathBuf::from(
            "C:/nonexistent-wtranscriber-workdir-zzz",
        ));
        assert!(migrate_for_platform(&mut cfg));
        assert!(cfg.last_dir.is_none());
    }

    #[test]
    fn migrate_keeps_existing_last_dir() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::default();
        cfg.last_dir = Some(dir.path().to_path_buf());
        migrate_for_platform(&mut cfg);
        assert_eq!(cfg.last_dir.as_deref(), Some(dir.path()));
    }

    #[test]
    fn device_as_str_matches_serde_label() {
        assert_eq!(Device::Cpu.as_str(), "cpu");
        assert_eq!(Device::Cuda.as_str(), "cuda");
    }

    #[test]
    fn default_asr_is_parakeet_v3() {
        let cfg = Config::default();
        assert_eq!(cfg.model, "parakeet-tdt-0.6b-v3-int8");
        assert!(matches!(cfg.engine, Engine::Parakeet));
    }

    #[test]
    fn engine_string_matches_catalog_values() {
        assert_eq!(Engine::Parakeet.as_str(), "parakeet");
        assert_eq!(Engine::NemoCtc.as_str(), "nemo-ctc");
        assert_eq!(Engine::Qwen3Asr.as_str(), "qwen3-asr");
        assert_eq!(Engine::WhisperCpp.as_str(), "whisper-cpp");
    }

    #[test]
    fn engine_from_str_roundtrip() {
        for e in [
            Engine::Parakeet,
            Engine::NemoCtc,
            Engine::Qwen3Asr,
            Engine::WhisperCpp,
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
    fn invalid_config_is_backed_up_and_reset_to_defaults() {
        let _g = crate::paths::PATHS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        crate::paths::clear_test_overrides();
        let tmp = tempfile::tempdir().unwrap();
        let config_file = tmp.path().join("config.yml");
        crate::paths::set_config_file(config_file.clone());
        std::fs::write(&config_file, "not-json").unwrap();

        let cfg = Config::load().expect("invalid config should reset to defaults");
        assert_eq!(cfg.model, Config::default().model);

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
        let reset = std::fs::read_to_string(&config_file).unwrap();
        assert!(reset.contains("parakeet-tdt-0.6b-v3-int8"));
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
    fn types_ts_mirrors_all_rust_enum_strings() {
        for file in ["types.ts", "schemas.ts"] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("src")
                .join(file);
            let raw = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            let mut missing: Vec<String> = Vec::new();
            let engines = [
                Engine::Parakeet,
                Engine::NemoCtc,
                Engine::Qwen3Asr,
                Engine::WhisperCpp,
            ];
            for e in engines {
                let needle = format!("\"{}\"", e.as_str());
                if !raw.contains(&needle) {
                    missing.push(format!("Engine::{e:?} expects {needle}"));
                }
            }
            let diarizers = [DiarizerChoice::SortformerOnnx, DiarizerChoice::Titanet];
            for d in diarizers {
                let serialised = serde_json::to_string(&d).unwrap();
                if !raw.contains(&serialised) {
                    missing.push(format!("DiarizerChoice::{d:?} expects {serialised}"));
                }
            }
            for d in [Device::Cpu, Device::Cuda] {
                let needle = format!("\"{}\"", d.as_str());
                if !raw.contains(&needle) {
                    missing.push(format!("Device::{d:?} expects {needle}"));
                }
            }
            assert!(
                missing.is_empty(),
                "src/{file} is out of sync with Rust enums:\n  {}",
                missing.join("\n  ")
            );
        }
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
