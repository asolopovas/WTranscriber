use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use serde::Serialize;

use crate::{
    audio,
    error::{Error, Result},
    transcriber::cache,
};

const AUDIO_EXTS: &[&str] = &[
    "wav", "wave", "mp3", "mp2", "mpga", "ogg", "oga", "ogv", "opus", "flac", "aac", "m4a", "m4b",
    "m4p", "m4r", "mp4", "m4v", "mov", "3gp", "3g2", "3gpp", "webm", "mkv", "mka", "avi", "wmv",
    "asf", "wma", "flv", "f4v", "f4a", "mpg", "mpeg", "ts", "mts", "m2ts", "vob", "aiff", "aif",
    "aifc", "au", "snd", "caf", "amr", "ac3", "eac3", "dts", "ape", "alac", "mpc", "wv", "tta",
    "ra", "rm", "rmvb", "voc", "gsm", "w64",
];

#[derive(Debug, Clone, Serialize)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_audio: bool,
    pub size_bytes: u64,
    pub modified_ms: i64,
    pub cache_key: Option<String>,
    pub utterances: Option<usize>,
    pub duration_ms: Option<u64>,
    pub trim_start_ms: Option<u64>,
    pub trim_end_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DirListing {
    pub path: PathBuf,
    pub parent: Option<PathBuf>,
    pub entries: Vec<DirEntry>,
}

pub fn list(path: &Path) -> Result<DirListing> {
    let abs = std::path::absolute(path)?;
    if !abs.is_dir() {
        return Err(Error::Config(format!("not a directory: {}", abs.display())));
    }

    let cache_index: Vec<cache::Entry> = cache::list();
    let cache_by_path: HashMap<&Path, &cache::Entry> = cache_index
        .iter()
        .map(|entry| (entry.source_path.as_path(), entry))
        .collect();

    let mut entries = Vec::new();
    for raw in std::fs::read_dir(&abs)? {
        let raw = raw?;
        let ft = raw.file_type()?;
        let name = raw.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = raw.path();
        let is_dir = ft.is_dir();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        let is_audio = !is_dir && AUDIO_EXTS.contains(&ext.as_str());

        if !is_dir && !is_audio {
            continue;
        }

        let meta = raw.metadata().ok();
        let size_bytes = meta.as_ref().map_or(0, std::fs::Metadata::len);
        let modified_ms = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(0));

        let mut cache_key = None;
        let mut utterances = None;
        let mut duration_ms = None;
        let mut trim_start_ms = None;
        let mut trim_end_ms = None;
        if is_audio {
            if let Some(c) = cache_by_path.get(path.as_path()) {
                cache_key = Some(c.key.clone());
                utterances = Some(c.utterances);
                duration_ms = Some(c.duration_ms);
            }
            if let Some(m) = audio::meta::load(&path) {
                if m.trim_start_ms > 0 {
                    trim_start_ms = Some(m.trim_start_ms);
                }
                trim_end_ms = m.trim_end_ms;
                if duration_ms.is_none() {
                    duration_ms = m.duration_ms;
                }
            }
        }

        entries.push(DirEntry {
            name,
            path,
            is_dir,
            is_audio,
            size_bytes,
            modified_ms,
            cache_key,
            utterances,
            duration_ms,
            trim_start_ms,
            trim_end_ms,
        });
    }

    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    let parent = abs.parent().map(Path::to_path_buf);

    Ok(DirListing {
        path: abs,
        parent,
        entries,
    })
}

pub fn home_dir() -> PathBuf {
    if let Some(p) = crate::paths::default_workdir_override() {
        let _ = std::fs::create_dir_all(&p);
        return p;
    }
    let base = directories::UserDirs::new().and_then(|u| {
        u.document_dir()
            .map(Path::to_path_buf)
            .or_else(|| Some(u.home_dir().to_path_buf()))
    });
    let candidate = base.map(|b| b.join("WTranscribe"));
    if let Some(dir) = candidate.as_ref()
        && std::fs::create_dir_all(dir).is_ok()
    {
        return dir.clone();
    }
    let fallback = crate::paths::data_dir().map_or_else(
        |_| std::env::temp_dir().join("WTranscribe"),
        |d| d.join("WTranscribe"),
    );
    let _ = std::fs::create_dir_all(&fallback);
    fallback
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use tempfile::TempDir;

    use super::*;
    use crate::transcriber::cache::{self as txc, Entry as CacheEntry};

    fn fresh_root() -> TempDir {
        let tmp = tempfile::tempdir().unwrap();
        crate::paths::init(
            tmp.path().join("config"),
            tmp.path().join("data"),
            tmp.path().join("cache"),
        );
        tmp
    }

    fn store_entry(key: &str, src: &Path) {
        let entry = CacheEntry {
            key: key.into(),
            source_path: std::path::absolute(src).unwrap(),
            source_name: src
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            model: "whisper".into(),
            language: "en".into(),
            speakers: 0,
            no_diarize: false,
            utterances: 1,
            duration_ms: 1000,
            created_at: Utc::now(),
            size_bytes: 0,
        };
        let transcript = crate::transcriber::Transcript {
            model: "whisper".into(),
            language: "en".into(),
            duration_ms: 1000,
            diarizer: Some(String::new()),
            device: None,
            speakers_detected: 0,
            utterances: vec![],
            words: vec![],
        };
        txc::store(entry, &transcript).unwrap();
    }

    #[test]
    fn cache_key_survives_rename() {
        let _g = crate::paths::PATHS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _root = fresh_root();

        let workdir = tempfile::tempdir().unwrap();
        let original = workdir.path().join("recording_260507-120738.wav");
        std::fs::write(&original, b"x").unwrap();
        store_entry("k-survive", &original);

        let renamed = workdir.path().join("named-topic_260507-150000.wav");
        std::fs::rename(&original, &renamed).unwrap();
        txc::rename_source(&original, &renamed).unwrap();

        let listing = list(workdir.path()).unwrap();
        let entry = listing
            .entries
            .iter()
            .find(|e| e.is_audio)
            .expect("audio entry expected");
        assert_eq!(entry.name, "named-topic_260507-150000.wav");
        assert_eq!(entry.cache_key.as_deref(), Some("k-survive"));
    }

    #[test]
    fn cache_key_is_null_without_rename_sync() {
        let _g = crate::paths::PATHS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _root = fresh_root();

        let workdir = tempfile::tempdir().unwrap();
        let original = workdir.path().join("recording_260507-120738.wav");
        std::fs::write(&original, b"x").unwrap();
        store_entry("k-stale", &original);

        let renamed = workdir.path().join("renamed.wav");
        std::fs::rename(&original, &renamed).unwrap();

        let listing = list(workdir.path()).unwrap();
        let entry = listing
            .entries
            .iter()
            .find(|e| e.is_audio)
            .expect("audio entry expected");
        assert!(
            entry.cache_key.is_none(),
            "without rename_source, lookup must miss (this is the original bug)"
        );
    }
}
