use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{
    error::{Error, Result},
    transcriber::cache,
};

const AUDIO_EXTS: &[&str] = &["wav", "mp3", "ogg", "m4a", "flac", "opus", "webm", "aac", "wma"];

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
        return Err(Error::Config(format!(
            "not a directory: {}",
            abs.display()
        )));
    }

    let cache_index: Vec<cache::Entry> = cache::list();

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
        if is_audio {
            for c in &cache_index {
                if c.source_path == path {
                    cache_key = Some(c.key.clone());
                    utterances = Some(c.utterances);
                    duration_ms = Some(c.duration_ms);
                    break;
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
        });
    }

    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));

    let parent = abs.parent().map(Path::to_path_buf);

    Ok(DirListing {
        path: abs,
        parent,
        entries,
    })
}

pub fn home_dir() -> PathBuf {
    if let Some(u) = directories::UserDirs::new() {
        if let Some(a) = u.audio_dir() {
            return a.to_path_buf();
        }
        return u.home_dir().to_path_buf();
    }
    PathBuf::from(".")
}
