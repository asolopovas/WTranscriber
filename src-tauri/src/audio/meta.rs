use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AudioMeta {
    #[serde(default)]
    pub trim_start_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trim_end_ms: Option<u64>,
}

impl AudioMeta {
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.trim_start_ms == 0 && self.trim_end_ms.is_none()
    }
}

#[must_use]
pub fn meta_path(audio: &Path) -> PathBuf {
    let mut s = audio.as_os_str().to_owned();
    s.push(".wtmeta.json");
    PathBuf::from(s)
}

pub fn load(audio: &Path) -> Option<AudioMeta> {
    let p = meta_path(audio);
    let raw = std::fs::read_to_string(&p).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn save(audio: &Path, meta: &AudioMeta) -> Result<()> {
    let p = meta_path(audio);
    if meta.is_default() {
        if p.exists() {
            std::fs::remove_file(&p)?;
        }
        return Ok(());
    }
    let raw = serde_json::to_string_pretty(meta)?;
    std::fs::write(&p, raw)?;
    Ok(())
}
