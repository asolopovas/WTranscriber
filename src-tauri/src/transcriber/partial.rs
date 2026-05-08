use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{
    error::Result,
    transcriber::{cache::transcript_path, transcript::Segment},
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Partial {
    pub key: String,
    #[serde(default)]
    pub last_done_sec: f64,
    #[serde(default)]
    pub segments: Vec<Segment>,
}

fn partial_path(key: &str) -> Result<PathBuf> {
    let p = transcript_path(key)?;
    let mut s = p.into_os_string();
    s.push(".partial.json");
    Ok(PathBuf::from(s))
}

#[must_use]
pub fn load(key: &str) -> Option<Partial> {
    let path = partial_path(key).ok()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn save(p: &Partial) -> Result<()> {
    let path = partial_path(&p.key)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string(p)?;
    let mut tmp = path.clone().into_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, raw)?;
    if let Err(e) = std::fs::rename(&tmp, &path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

pub fn clear(key: &str) -> Result<()> {
    let path = partial_path(key)?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}
