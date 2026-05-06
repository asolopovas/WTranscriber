use serde::{Deserialize, Serialize};

use crate::{error::Result, paths};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub family: ModelFamily,
    pub installed: bool,
    pub size_mb: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelFamily {
    Whisper,
    Parakeet,
    Canary,
    Nemo,
}

pub fn list() -> Result<Vec<ModelInfo>> {
    let root = paths::models_dir()?;
    let mut out = Vec::new();
    if root.exists() {
        for entry in std::fs::read_dir(&root)? {
            let entry = entry?;
            let id = entry.file_name().to_string_lossy().into_owned();
            out.push(ModelInfo {
                id,
                family: ModelFamily::Whisper,
                installed: true,
                size_mb: None,
            });
        }
    }
    Ok(out)
}
