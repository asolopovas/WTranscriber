use std::{path::PathBuf, sync::LazyLock};

use serde::{Deserialize, Serialize};

use crate::{error::Result, paths};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    Asr,
    Diarizer,
    Llm,
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
    pub files: Vec<FileSpec>,
}

static CATALOG: LazyLock<Vec<Entry>> = LazyLock::new(default_catalog);

pub fn catalog() -> &'static [Entry] {
    &CATALOG
}

pub fn by_id(id: &str) -> Option<&'static Entry> {
    CATALOG.iter().find(|e| e.id == id)
}

#[allow(dead_code)]
pub fn by_family(family: Family) -> Vec<&'static Entry> {
    CATALOG.iter().filter(|e| e.family == family).collect()
}

#[allow(dead_code)]
pub fn default_id(family: Family) -> Option<&'static str> {
    by_family(family)
        .into_iter()
        .find(|e| e.default_active)
        .or_else(|| by_family(family).into_iter().next())
        .map(|e| e.id.as_str())
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

fn default_catalog() -> Vec<Entry> {
    vec![
        Entry {
            id: "sherpa-whisper-turbo".into(),
            family: Family::Asr,
            engine: "whisper-onnx".into(),
            display_name: "Whisper large-v3-turbo (ONNX, multilingual)".into(),
            description: "OpenAI Whisper large-v3-turbo via ONNX Runtime, 99 languages.".into(),
            languages: vec!["auto", "en", "de", "fr", "es", "it", "pt", "nl", "pl", "ru", "uk", "zh", "ja", "ko", "ar", "tr", "hi"]
                .into_iter().map(String::from).collect(),
            size_bytes: 1_036_613_791,
            default_active: true,
            files: vec![
                FileSpec {
                    url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-turbo/resolve/main/turbo-encoder.int8.onnx".into(),
                    rel_path: "sherpa-whisper-turbo/turbo-encoder.int8.onnx".into(),
                    size_bytes: 674_716_297,
                    sha256: "b02dcdf54f348741e93fe732b67d933c8dcb6735655f710640143081db38878b".into(),
                },
                FileSpec {
                    url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-turbo/resolve/main/turbo-decoder.int8.onnx".into(),
                    rel_path: "sherpa-whisper-turbo/turbo-decoder.int8.onnx".into(),
                    size_bytes: 361_080_764,
                    sha256: "20accd02388482eb3a46bd615631adfdc85e1eb2c7db9ea3f02a40ffe6b81547".into(),
                },
                FileSpec {
                    url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-turbo/resolve/main/turbo-tokens.txt".into(),
                    rel_path: "sherpa-whisper-turbo/turbo-tokens.txt".into(),
                    size_bytes: 816_730,
                    sha256: "b34b360dbb493e781e479794586d661700670d65564001f23024971d1f2fa126".into(),
                },
            ],
        },
        Entry {
            id: "qwen3-0.6b-q4km".into(),
            family: Family::Llm,
            engine: "llama-cli".into(),
            display_name: "Qwen3 0.6B (Q4_K_M, namer)".into(),
            description: "Compact GGUF used by the auto-rename feature.".into(),
            languages: Vec::new(),
            size_bytes: 396_705_472,
            default_active: true,
            files: vec![FileSpec {
                url: "https://huggingface.co/unsloth/Qwen3-0.6B-GGUF/resolve/main/Qwen3-0.6B-Q4_K_M.gguf".into(),
                rel_path: "qwen3-0.6b-q4km.gguf".into(),
                size_bytes: 396_705_472,
                sha256: "ac2d97712095a558e31573f62f466a3f9d93990898b0ec79d7c974c1780d524a".into(),
            }],
        },
        Entry {
            id: "sherpa-pyannote-titanet".into(),
            family: Family::Diarizer,
            engine: "sherpa".into(),
            display_name: "pyannote-3.0 segmentation + TitaNet-Large".into(),
            description: "sherpa-onnx pyannote-3.0 segmentation with NeMo TitaNet embeddings.".into(),
            languages: Vec::new(),
            size_bytes: 0,
            default_active: true,
            files: vec![
                FileSpec {
                    url: "https://huggingface.co/csukuangfj/sherpa-onnx-pyannote-segmentation-3-0/resolve/main/model.onnx".into(),
                    rel_path: "sherpa-onnx-pyannote-segmentation-3-0/model.onnx".into(),
                    size_bytes: 0,
                    sha256: String::new(),
                },
                FileSpec {
                    url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/nemo_en_titanet_large.onnx".into(),
                    rel_path: "titanet_large.onnx".into(),
                    size_bytes: 0,
                    sha256: String::new(),
                },
            ],
        },
    ]
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
    fn diarizer_family_has_one_entry() {
        assert_eq!(by_family(Family::Diarizer).len(), 1);
    }
}
