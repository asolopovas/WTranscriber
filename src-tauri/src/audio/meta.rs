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
    pub const fn is_default(&self) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_path_appends_sidecar_extension() {
        let p = meta_path(Path::new("/audio/clip.wav"));
        assert!(
            p.to_string_lossy().ends_with("clip.wav.wtmeta.json"),
            "got {}",
            p.display()
        );
    }

    #[test]
    fn default_meta_is_default() {
        assert!(AudioMeta::default().is_default());
    }

    #[test]
    fn nonzero_trim_is_not_default() {
        let m = AudioMeta {
            trim_start_ms: 1,
            trim_end_ms: None,
        };
        assert!(!m.is_default());
    }

    #[test]
    fn save_persists_non_default_and_load_returns_it() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("clip.wav");
        std::fs::write(&audio, b"x").unwrap();
        let meta = AudioMeta {
            trim_start_ms: 250,
            trim_end_ms: Some(9_000),
        };
        save(&audio, &meta).unwrap();
        let loaded = load(&audio).unwrap();
        assert_eq!(loaded.trim_start_ms, 250);
        assert_eq!(loaded.trim_end_ms, Some(9_000));
    }

    #[test]
    fn save_default_removes_existing_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("clip.wav");
        std::fs::write(&audio, b"x").unwrap();
        save(
            &audio,
            &AudioMeta {
                trim_start_ms: 1,
                trim_end_ms: None,
            },
        )
        .unwrap();
        assert!(meta_path(&audio).exists());
        save(&audio, &AudioMeta::default()).unwrap();
        assert!(!meta_path(&audio).exists());
    }

    #[test]
    fn save_default_is_noop_when_no_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("clip.wav");
        save(&audio, &AudioMeta::default()).unwrap();
        assert!(load(&audio).is_none());
    }
}
