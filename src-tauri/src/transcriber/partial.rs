use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{
    error::Result,
    fs_utils,
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
    fs_utils::ensure_parent_dir(&path)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_root() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        crate::paths::init(
            tmp.path().join("config"),
            tmp.path().join("data"),
            tmp.path().join("cache"),
        );
        tmp
    }

    fn sample(key: &str) -> Partial {
        Partial {
            key: key.into(),
            last_done_sec: 12.5,
            segments: vec![Segment {
                text: "hello".into(),
                start_ms: 0,
                end_ms: 500,
                tokens: Vec::new(),
            }],
        }
    }

    #[test]
    fn save_and_load_roundtrip() {
        let _g = crate::paths::PATHS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _root = fresh_root();
        let p = sample("kpartial");
        save(&p).unwrap();
        let loaded = load("kpartial").expect("load returns saved partial");
        assert_eq!(loaded.key, "kpartial");
        assert!((loaded.last_done_sec - 12.5).abs() < 1e-9);
        assert_eq!(loaded.segments.len(), 1);
    }

    #[test]
    fn load_returns_none_when_missing() {
        let _g = crate::paths::PATHS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _root = fresh_root();
        assert!(load("never-saved").is_none());
    }

    #[test]
    fn clear_removes_persisted_file() {
        let _g = crate::paths::PATHS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _root = fresh_root();
        let p = sample("kclear");
        save(&p).unwrap();
        clear("kclear").unwrap();
        assert!(load("kclear").is_none());
    }

    #[test]
    fn clear_is_idempotent_on_missing_key() {
        let _g = crate::paths::PATHS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _root = fresh_root();
        clear("absent").unwrap();
    }
}
