use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

use serde::{Deserialize, Serialize};

use crate::error::Result;

static META_LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    META_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[allow(clippy::struct_field_names)]
pub struct AudioMeta {
    #[serde(default)]
    pub trim_start_ms: u64,
    #[serde(default)]
    pub trim_end_ms: Option<u64>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

impl AudioMeta {
    #[must_use]
    pub const fn is_default(&self) -> bool {
        self.trim_start_ms == 0 && self.trim_end_ms.is_none() && self.duration_ms.is_none()
    }
}

#[must_use]
pub fn meta_path(audio: &Path) -> PathBuf {
    let mut name = audio.file_name().unwrap_or_default().to_owned();
    name.push(".wtmeta.json");
    audio
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(".meta")
        .join(name)
}

fn legacy_meta_path(audio: &Path) -> PathBuf {
    let mut s = audio.as_os_str().to_owned();
    s.push(".wtmeta.json");
    PathBuf::from(s)
}

pub fn load(audio: &Path) -> Option<AudioMeta> {
    load_checked(audio).ok().flatten()
}

pub fn load_checked(audio: &Path) -> Result<Option<AudioMeta>> {
    let _guard = lock();
    load_unlocked(audio)
}

fn load_unlocked(audio: &Path) -> Result<Option<AudioMeta>> {
    let raw = match std::fs::read_to_string(meta_path(audio)) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            match std::fs::read_to_string(legacy_meta_path(audio)) {
                Ok(raw) => raw,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(err) => return Err(err.into()),
            }
        }
        Err(err) => return Err(err.into()),
    };
    Ok(Some(serde_json::from_str(&raw)?))
}

pub fn update_duration(audio: &Path, duration_ms: u64) -> Result<()> {
    let _guard = lock();
    let mut meta = load_unlocked(audio)?.unwrap_or_default();
    if meta.duration_ms != Some(duration_ms) {
        meta.duration_ms = Some(duration_ms);
        save_unlocked(audio, &meta)?;
    }
    Ok(())
}

pub fn save_trim(audio: &Path, trim: &AudioMeta) -> Result<()> {
    let _guard = lock();
    let mut meta = load_unlocked(audio)?.unwrap_or_default();
    meta.trim_start_ms = trim.trim_start_ms;
    meta.trim_end_ms = trim.trim_end_ms;
    save_unlocked(audio, &meta)
}

fn remove_if_exists(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

pub fn rename(source: &Path, destination: &Path) -> Result<()> {
    let _guard = lock();
    let cached = meta_path(source);
    let legacy = legacy_meta_path(source);
    let from = if cached.exists() { &cached } else { &legacy };
    if !from.exists() {
        return Ok(());
    }
    let to = meta_path(destination);
    crate::fs_utils::ensure_parent_dir(&to)?;
    std::fs::rename(from, to)?;
    remove_if_exists(&legacy)
}

pub fn save(audio: &Path, meta: &AudioMeta) -> Result<()> {
    let _guard = lock();
    save_unlocked(audio, meta)
}

fn save_unlocked(audio: &Path, meta: &AudioMeta) -> Result<()> {
    let p = meta_path(audio);
    if meta.is_default() {
        remove_if_exists(&legacy_meta_path(audio))?;
        remove_if_exists(&p)?;
        return Ok(());
    }
    let raw = serde_json::to_string_pretty(meta)?;
    crate::fs_utils::ensure_parent_dir(&p)?;
    let mut pending = tempfile::NamedTempFile::new_in(p.parent().expect("metadata has a parent"))?;
    pending.write_all(raw.as_bytes())?;
    pending.persist(&p).map_err(|err| err.error)?;
    remove_if_exists(&legacy_meta_path(audio))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_path_uses_meta_subdirectory_and_preserves_audio_extension() {
        let p = meta_path(Path::new("/audio/clip.wav"));
        assert_eq!(p, Path::new("/audio/.meta/clip.wav.wtmeta.json"));
        assert_eq!(
            meta_path(Path::new("clip.mp3")),
            Path::new(".meta/clip.mp3.wtmeta.json")
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
            ..AudioMeta::default()
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
            ..AudioMeta::default()
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
                ..AudioMeta::default()
            },
        )
        .unwrap();
        assert!(meta_path(&audio).exists());
        save(&audio, &AudioMeta::default()).unwrap();
        assert!(!meta_path(&audio).exists());
    }

    #[test]
    fn duration_only_meta_persists_and_loads() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("clip.wav");
        std::fs::write(&audio, b"x").unwrap();
        save(
            &audio,
            &AudioMeta {
                duration_ms: Some(12_345),
                ..AudioMeta::default()
            },
        )
        .unwrap();
        let loaded = load(&audio).unwrap();
        assert_eq!(loaded.duration_ms, Some(12_345));
        assert_eq!(loaded.trim_start_ms, 0);
        assert_eq!(loaded.trim_end_ms, None);
    }

    #[test]
    fn save_default_is_noop_when_no_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("clip.wav");
        save(&audio, &AudioMeta::default()).unwrap();
        assert!(load(&audio).is_none());
        assert!(!dir.path().join(".meta").exists());
    }

    #[test]
    fn legacy_metadata_is_read_and_migrated_on_save() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("clip.wav");
        let legacy = legacy_meta_path(&audio);
        std::fs::write(
            &legacy,
            r#"{"trim_start_ms":250,"trim_end_ms":9000,"duration_ms":12000}"#,
        )
        .unwrap();
        let meta = load(&audio).unwrap();
        assert_eq!(meta.trim_start_ms, 250);
        save(&audio, &meta).unwrap();
        assert!(!legacy.exists());
        assert!(meta_path(&audio).exists());
        assert_eq!(load(&audio).unwrap().duration_ms, Some(12000));
    }

    #[test]
    fn metadata_subdirectory_takes_precedence_and_reset_removes_both_locations() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("clip.wav");
        save(
            &audio,
            &AudioMeta {
                trim_start_ms: 500,
                ..AudioMeta::default()
            },
        )
        .unwrap();
        std::fs::write(legacy_meta_path(&audio), r#"{"trim_start_ms":250}"#).unwrap();
        assert_eq!(load(&audio).unwrap().trim_start_ms, 500);
        save(&audio, &AudioMeta::default()).unwrap();
        assert!(load(&audio).is_none());
        assert!(!meta_path(&audio).exists());
        assert!(!legacy_meta_path(&audio).exists());
    }

    #[test]
    fn failed_metadata_write_preserves_legacy_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("clip.wav");
        std::fs::write(legacy_meta_path(&audio), r#"{"trim_start_ms":250}"#).unwrap();
        std::fs::write(dir.path().join(".meta"), b"not a directory").unwrap();
        assert!(
            save(
                &audio,
                &AudioMeta {
                    trim_start_ms: 500,
                    ..AudioMeta::default()
                }
            )
            .is_err()
        );
        assert_eq!(
            std::fs::read_to_string(legacy_meta_path(&audio)).unwrap(),
            r#"{"trim_start_ms":250}"#
        );
    }
    #[test]
    fn duration_updates_and_trim_saves_preserve_each_other() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("clip.wav");
        std::thread::scope(|scope| {
            scope.spawn(|| {
                for duration in 10000..10050 {
                    update_duration(&source, duration).unwrap();
                }
            });
            scope.spawn(|| {
                for start in 1000..1050 {
                    save_trim(
                        &source,
                        &AudioMeta {
                            trim_start_ms: start,
                            trim_end_ms: Some(9000),
                            duration_ms: None,
                        },
                    )
                    .unwrap();
                }
            });
        });
        let meta = load(&source).unwrap();
        assert_eq!(meta.trim_start_ms, 1049);
        assert_eq!(meta.trim_end_ms, Some(9000));
        assert_eq!(meta.duration_ms, Some(10049));
    }

    #[test]
    fn probe_update_does_not_replace_invalid_metadata_with_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("clip.wav");
        crate::fs_utils::ensure_parent_dir(&meta_path(&source)).unwrap();
        std::fs::write(meta_path(&source), b"invalid json").unwrap();
        assert!(update_duration(&source, 10000).is_err());
        assert_eq!(std::fs::read(meta_path(&source)).unwrap(), b"invalid json");
    }
}
