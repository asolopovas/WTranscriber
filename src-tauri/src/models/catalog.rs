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

static CATALOG: LazyLock<Vec<Entry>> = LazyLock::new(default_catalog);

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

#[allow(clippy::too_many_lines)]
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
            android_default: false,
            desktop_only: false,
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
            id: "parakeet-tdt-0.6b-v3-int8".into(),
            family: Family::Asr,
            engine: "parakeet".into(),
            display_name: "Parakeet TDT 0.6B v3 (25 EU langs)".into(),
            description: "NVIDIA Parakeet TDT v3, 25 European languages. ~671 MB.".into(),
            languages: vec!["bg","hr","cs","da","nl","en","et","fi","fr","de","el","hu","it","lv","lt","mt","pl","pt","ro","sk","sl","es","sv","ru","uk"]
                .into_iter().map(String::from).collect(),
            size_bytes: 670_478_772,
            default_active: false,
            android_default: true,
            desktop_only: false,
            files: vec![
                FileSpec {
                    url: "https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/resolve/main/encoder.int8.onnx".into(),
                    rel_path: "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/encoder.int8.onnx".into(),
                    size_bytes: 652_184_281,
                    sha256: "acfc2b4456377e15d04f0243af540b7fe7c992f8d898d751cf134c3a55fd2247".into(),
                },
                FileSpec {
                    url: "https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/resolve/main/decoder.int8.onnx".into(),
                    rel_path: "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/decoder.int8.onnx".into(),
                    size_bytes: 11_845_275,
                    sha256: "179e50c43d1a9de79c8a24149a2f9bac6eb5981823f2a2ed88d655b24248db4e".into(),
                },
                FileSpec {
                    url: "https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/resolve/main/joiner.int8.onnx".into(),
                    rel_path: "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/joiner.int8.onnx".into(),
                    size_bytes: 6_355_277,
                    sha256: "3164c13fc2821009440d20fcb5fdc78bff28b4db2f8d0f0b329101719c0948b3".into(),
                },
                FileSpec {
                    url: "https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/resolve/main/tokens.txt".into(),
                    rel_path: "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/tokens.txt".into(),
                    size_bytes: 93_939,
                    sha256: "d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d".into(),
                },
            ],
        },
        Entry {
            id: "gigaam-v3-ru".into(),
            family: Family::Asr,
            engine: "nemo-ctc".into(),
            display_name: "GigaAM v3 (Russian)".into(),
            description: "Sber GigaAM v3 NeMo CTC, Russian-only. Fast and accurate. ~225 MB.".into(),
            languages: vec!["ru".into()],
            size_bytes: 224_721_672,
            default_active: false,
            android_default: false,
            desktop_only: false,
            files: vec![
                FileSpec {
                    url: "https://huggingface.co/csukuangfj/sherpa-onnx-nemo-ctc-giga-am-v3-russian-2025-12-16/resolve/main/model.int8.onnx".into(),
                    rel_path: "sherpa-onnx-nemo-ctc-giga-am-v3-russian-2025-12-16/model.int8.onnx".into(),
                    size_bytes: 224_721_476,
                    sha256: "f86ebfa0429ced91be6054fc344827e9c6c2572f3c318416cd974b06f66437ec".into(),
                },
                FileSpec {
                    url: "https://huggingface.co/csukuangfj/sherpa-onnx-nemo-ctc-giga-am-v3-russian-2025-12-16/resolve/main/tokens.txt".into(),
                    rel_path: "sherpa-onnx-nemo-ctc-giga-am-v3-russian-2025-12-16/tokens.txt".into(),
                    size_bytes: 196,
                    sha256: "17cc514451bcceac9c280068c71502f8448f99e9fb1456b8d0761651fd0392f2".into(),
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
            android_default: true,
            desktop_only: false,
            files: vec![FileSpec {
                url: "https://huggingface.co/unsloth/Qwen3-0.6B-GGUF/resolve/main/Qwen3-0.6B-Q4_K_M.gguf".into(),
                rel_path: "qwen3-0.6b-q4km.gguf".into(),
                size_bytes: 396_705_472,
                sha256: "ac2d97712095a558e31573f62f466a3f9d93990898b0ec79d7c974c1780d524a".into(),
            }],
        },
        Entry {
            id: "qwen3-1.7b-q4km".into(),
            family: Family::Llm,
            engine: "llama-cli".into(),
            display_name: "Qwen3 1.7B (Q4_K_M, namer)".into(),
            description: "Larger, slightly higher-quality naming. Slower on phone.".into(),
            languages: Vec::new(),
            size_bytes: 1_107_409_472,
            default_active: false,
            android_default: false,
            desktop_only: false,
            files: vec![FileSpec {
                url: "https://huggingface.co/unsloth/Qwen3-1.7B-GGUF/resolve/main/Qwen3-1.7B-Q4_K_M.gguf".into(),
                rel_path: "qwen3-1.7b-q4km.gguf".into(),
                size_bytes: 1_107_409_472,
                sha256: "b139949c5bd74937ad8ed8c8cf3d9ffb1e99c866c823204dc42c0d91fa181897".into(),
            }],
        },
        Entry {
            id: "nemo-sortformer-v2".into(),
            family: Family::Diarizer,
            engine: "nemo-sortformer".into(),
            display_name: "NVIDIA NeMo Sortformer 4-speaker v2".into(),
            description: "GPU-first NVIDIA NeMo diarization used by desktop transcription jobs.".into(),
            languages: Vec::new(),
            size_bytes: 0,
            default_active: true,
            android_default: false,
            desktop_only: true,
            files: Vec::new(),
        },
        Entry {
            id: "diar-eres2net-base".into(),
            family: Family::Diarizer,
            engine: "sherpa".into(),
            display_name: "pyannote-3.0 + 3D-Speaker ERes2Net-base".into(),
            description: "Higher-quality CPU diarizer for mobile. Newer ERes2Net architecture, multilingual zh+en. ~46 MB.".into(),
            languages: Vec::new(),
            size_bytes: 45_586_678,
            default_active: false,
            android_default: true,
            desktop_only: false,
            files: vec![
                FileSpec {
                    url: "https://huggingface.co/csukuangfj/sherpa-onnx-pyannote-segmentation-3-0/resolve/main/model.onnx".into(),
                    rel_path: "sherpa-onnx-pyannote-segmentation-3-0/model.onnx".into(),
                    size_bytes: 5_992_913,
                    sha256: "220ad67ca923bef2fa91f2390c786097bf305bceb5e261d4af67b38e938e1079".into(),
                },
                FileSpec {
                    url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_eres2net_base_sv_zh-cn_3dspeaker_16k.onnx".into(),
                    rel_path: "3dspeaker_eres2net_base.onnx".into(),
                    size_bytes: 39_593_761,
                    sha256: "1a331345f04805badbb495c775a6ddffcdd1a732567d5ec8b3d5749e3c7a5e4b".into(),
                },
            ],
        },
        Entry {
            id: "sherpa-pyannote-titanet".into(),
            family: Family::Diarizer,
            engine: "sherpa".into(),
            display_name: "pyannote-3.0 segmentation + TitaNet-Large".into(),
            description: "CPU-compatible fallback diarizer with pyannote segmentation and NeMo TitaNet embeddings.".into(),
            languages: Vec::new(),
            size_bytes: 0,
            default_active: false,
            android_default: false,
            desktop_only: false,
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
    fn diarizer_family_has_nemo_and_fallback() {
        assert!(by_family(Family::Diarizer).len() >= 2);
    }

    #[test]
    #[cfg(not(target_os = "android"))]
    fn default_diarizer_is_nemo_sortformer() {
        assert_eq!(default_id(Family::Diarizer), Some("nemo-sortformer-v2"));
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
