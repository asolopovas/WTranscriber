use std::{path::PathBuf, sync::LazyLock};

use serde::{Deserialize, Serialize};

use crate::{config::Engine, error::Result, paths};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    Asr,
    Diarizer,
    Llm,
}

impl Family {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Asr => "asr",
            Self::Diarizer => "diarizer",
            Self::Llm => "llm",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSpec {
    pub url: String,
    pub rel_path: String,
    #[serde(default)]
    pub size_bytes: u64,
    #[serde(default)]
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub family: Family,
    #[serde(default)]
    pub engine: String,
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub size_bytes: u64,
    #[serde(default)]
    pub default_active: bool,
    #[serde(default)]
    pub android_default: bool,
    #[serde(default)]
    pub desktop_only: bool,
    pub files: Vec<FileSpec>,
}

impl Entry {
    #[must_use]
    pub fn engine_kind(&self) -> Option<Engine> {
        self.engine.parse().ok()
    }
}

static CATALOG: LazyLock<Vec<Entry>> = LazyLock::new(super::catalog_data::default_catalog);

const IS_ANDROID: bool = cfg!(target_os = "android");

const fn visible(e: &Entry) -> bool {
    !(IS_ANDROID && e.desktop_only)
}

pub fn catalog() -> Vec<&'static Entry> {
    CATALOG.iter().filter(|e| visible(e)).collect()
}

pub fn by_id(id: &str) -> Option<&'static Entry> {
    CATALOG.iter().find(|e| e.id == id && visible(e))
}

#[allow(dead_code)]
pub fn by_family(family: Family) -> Vec<&'static Entry> {
    CATALOG
        .iter()
        .filter(|e| e.family == family && visible(e))
        .collect()
}

pub fn default_id(family: Family) -> Option<&'static str> {
    let list = by_family(family);
    if IS_ANDROID && let Some(e) = list.iter().find(|e| e.android_default) {
        return Some(e.id.as_str());
    }
    list.iter()
        .find(|e| e.default_active)
        .or_else(|| list.first())
        .map(|e| e.id.as_str())
}

pub fn model_dir(model_id: &str) -> Result<PathBuf> {
    let root = paths::models_dir()?;
    let Some(entry) = by_id(model_id) else {
        return Ok(root.join(model_id));
    };
    let segment = entry
        .files
        .iter()
        .find_map(|f| f.rel_path.split('/').next().filter(|s| !s.is_empty()))
        .unwrap_or(&entry.id);
    Ok(root.join(segment))
}

pub fn paths_for(entry: &Entry) -> Result<Vec<PathBuf>> {
    let root = paths::models_dir()?;
    Ok(entry
        .files
        .iter()
        .map(|f| {
            root.join(
                f.rel_path
                    .replace('/', std::path::MAIN_SEPARATOR_STR.as_ref()),
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_catalog_has_whisper_turbo() {
        assert!(by_id("sherpa-whisper-turbo").is_some());
    }

    #[test]
    fn default_asr_is_whisper_turbo() {
        assert_eq!(default_id(Family::Asr), Some("sherpa-whisper-turbo"));
    }

    #[test]
    fn diarizer_family_has_nemo_and_fallback() {
        assert!(by_family(Family::Diarizer).len() >= 2);
    }

    #[test]
    #[cfg(not(target_os = "android"))]
    fn default_diarizer_is_sortformer_onnx() {
        assert_eq!(
            default_id(Family::Diarizer),
            Some("sortformer-v2-onnx-4spk")
        );
    }

    #[test]
    fn by_id_returns_none_for_unknown() {
        assert!(by_id("does-not-exist").is_none());
    }

    #[test]
    fn family_as_str_matches_serde_lowercase() {
        assert_eq!(Family::Asr.as_str(), "asr");
        assert_eq!(Family::Diarizer.as_str(), "diarizer");
        assert_eq!(Family::Llm.as_str(), "llm");
    }

    #[test]
    fn engine_kind_matches_catalog_engine_string() {
        let entry = by_id("sherpa-whisper-turbo").unwrap();
        assert_eq!(entry.engine_kind().unwrap().as_str(), entry.engine);
    }

    #[test]
    fn paths_for_resolves_relative_files_under_models_dir() {
        let entry = by_id("qwen3-0.6b-q4km").unwrap();
        let paths = paths_for(entry).unwrap();
        assert_eq!(paths.len(), entry.files.len());
        for p in paths {
            assert!(p.ends_with("qwen3-0.6b-q4km.gguf"));
        }
    }

    #[test]
    fn model_dir_uses_first_segment_of_rel_path() {
        let dir = model_dir("sherpa-whisper-turbo").unwrap();
        assert!(dir.ends_with("sherpa-whisper-turbo"));
    }

    #[test]
    fn asr_family_contains_whisper_and_parakeet() {
        let ids: Vec<&str> = by_family(Family::Asr)
            .iter()
            .map(|e| e.id.as_str())
            .collect();
        assert!(ids.contains(&"sherpa-whisper-turbo"));
        assert!(ids.contains(&"parakeet-tdt-0.6b-v3-int8"));
    }

    #[test]
    fn llm_family_includes_qwen3_default() {
        assert!(
            by_family(Family::Llm)
                .iter()
                .any(|e| e.id == "qwen3-0.6b-q4km")
        );
    }
}
