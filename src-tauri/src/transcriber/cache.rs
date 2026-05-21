use std::{
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{error::Result, paths, transcriber::Transcript};

static INDEX_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

fn lock_index() -> std::sync::MutexGuard<'static, ()> {
    INDEX_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn absolute_or_clone(p: &Path) -> PathBuf {
    std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub key: String,
    pub source_path: PathBuf,
    pub source_name: String,
    pub model: String,
    pub language: String,
    #[serde(default)]
    pub speakers: u32,
    #[serde(default)]
    pub no_diarize: bool,
    pub utterances: usize,
    #[serde(default)]
    pub duration_ms: u64,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub size_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct KeyParams {
    pub source_path: PathBuf,
    pub mtime_ns: u128,
    pub model: String,
    pub language: String,
    pub speakers: u32,
    pub no_diarize: bool,
    pub trim_start_ms: u64,
    pub trim_end_ms: u64,
    pub precise_word_timestamps: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct KeyOptions<'a> {
    pub model: &'a str,
    pub language: &'a str,
    pub speakers: u32,
    pub no_diarize: bool,
    pub trim_start_ms: u64,
    pub trim_end_ms: u64,
    pub precise_word_timestamps: bool,
}

fn cache_root() -> Result<PathBuf> {
    let d = paths::cache_dir()?.join("transcripts");
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

fn index_path() -> Result<PathBuf> {
    Ok(cache_root()?.join("index.json"))
}

pub fn transcript_path(key: &str) -> Result<PathBuf> {
    Ok(cache_root()?.join(format!("{key}.json")))
}

pub fn build_key_params(source_path: &Path, options: KeyOptions<'_>) -> Result<KeyParams> {
    let abs = std::path::absolute(source_path)?;
    let meta = std::fs::metadata(&abs)?;
    let mtime_ns = meta
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    Ok(KeyParams {
        source_path: abs,
        mtime_ns,
        model: options.model.to_owned(),
        language: options.language.to_owned(),
        speakers: options.speakers,
        no_diarize: options.no_diarize,
        trim_start_ms: options.trim_start_ms,
        trim_end_ms: options.trim_end_ms,
        precise_word_timestamps: options.precise_word_timestamps,
    })
}

#[must_use]
pub fn compute_key(p: &KeyParams) -> String {
    let s = format!(
        "{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}",
        p.source_path.display(),
        p.mtime_ns,
        p.model,
        p.language,
        p.speakers,
        p.no_diarize,
        p.trim_start_ms,
        p.trim_end_ms,
        p.precise_word_timestamps,
    );
    let hash = Sha256::digest(s.as_bytes());
    hex::encode(&hash[..16])
}

fn load_index() -> Vec<Entry> {
    let Ok(path) = index_path() else {
        return Vec::new();
    };
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn save_index(entries: &[Entry]) -> Result<()> {
    let path = index_path()?;
    let raw = serde_json::to_string_pretty(entries)?;
    std::fs::write(path, raw)?;
    Ok(())
}

pub fn load(key: &str) -> Result<Option<Transcript>> {
    let path = transcript_path(key)?;
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path)?;
    Ok(Some(serde_json::from_str(&raw)?))
}

pub fn overwrite_transcript(key: &str, transcript: &Transcript) -> Result<()> {
    let path = transcript_path(key)?;
    if !path.exists() {
        return Err(crate::error::Error::Config(format!(
            "no cached transcript for key {key}"
        )));
    }
    let raw = serde_json::to_vec_pretty(transcript)?;
    let size_bytes = raw.len() as u64;
    std::fs::write(&path, &raw)?;

    let _g = lock_index();
    let mut entries = load_index();
    let mut changed = false;
    for e in &mut entries {
        if e.key == key {
            e.size_bytes = size_bytes;
            changed = true;
        }
    }
    if changed {
        save_index(&entries)?;
    }
    Ok(())
}

pub fn store(mut entry: Entry, transcript: &Transcript) -> Result<PathBuf> {
    let path = transcript_path(&entry.key)?;
    let raw = serde_json::to_vec_pretty(transcript)?;
    entry.size_bytes = raw.len() as u64;
    std::fs::write(&path, &raw)?;

    let _g = lock_index();
    let mut entries: Vec<Entry> = load_index()
        .into_iter()
        .filter(|e| e.key != entry.key)
        .collect();
    entries.push(entry);
    save_index(&entries)?;
    Ok(path)
}

#[must_use]
pub fn list() -> Vec<Entry> {
    let mut entries = load_index();
    entries.sort_by_key(|e| std::cmp::Reverse(e.created_at));
    entries
}

pub fn rename_source(old_path: &Path, new_path: &Path) -> Result<()> {
    let old_abs = absolute_or_clone(old_path);
    let new_abs = absolute_or_clone(new_path);
    let new_name = new_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let _g = lock_index();
    let mut entries = load_index();
    let mut changed = false;
    for e in &mut entries {
        if e.source_path == old_abs {
            e.source_path.clone_from(&new_abs);
            if !new_name.is_empty() {
                e.source_name.clone_from(&new_name);
            }
            changed = true;
        }
    }
    if changed {
        save_index(&entries)?;
    }
    Ok(())
}

pub fn invalidate(key: &str) -> Result<()> {
    let path = transcript_path(key)?;
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    let _g = lock_index();
    let entries: Vec<Entry> = load_index().into_iter().filter(|e| e.key != key).collect();
    save_index(&entries)?;
    Ok(())
}

pub fn invalidate_for_source(source: &Path) -> Result<usize> {
    let abs = absolute_or_clone(source);
    let _g = lock_index();
    let mut removed = 0_usize;
    let mut keep: Vec<Entry> = Vec::new();
    for e in load_index() {
        if e.source_path == abs {
            if let Ok(p) = transcript_path(&e.key)
                && p.exists()
            {
                let _ = std::fs::remove_file(&p);
            }
            removed += 1;
        } else {
            keep.push(e);
        }
    }
    if removed > 0 {
        save_index(&keep)?;
    }
    Ok(removed)
}

pub fn clear_all() -> Result<u64> {
    let root = cache_root()?;
    let mut removed = 0_u64;
    for entry in std::fs::read_dir(&root)? {
        let path = entry?.path();
        if path.is_file() {
            std::fs::remove_file(path)?;
            removed += 1;
        }
    }
    save_index(&[])?;
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_is_deterministic() {
        let p = KeyParams {
            source_path: PathBuf::from("/audio/sample.wav"),
            mtime_ns: 12345,
            model: "whisper".into(),
            language: "en".into(),
            speakers: 0,
            no_diarize: false,
            trim_start_ms: 0,
            trim_end_ms: 0,
            precise_word_timestamps: false,
        };
        let k1 = compute_key(&p);
        let k2 = compute_key(&p);
        assert_eq!(k1, k2);
        assert_eq!(k1.len(), 32);
    }

    #[test]
    fn build_key_params_reads_file_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip.wav");
        std::fs::write(&path, b"hello").unwrap();
        let params = build_key_params(
            &path,
            KeyOptions {
                model: "whisper",
                language: "en",
                speakers: 0,
                no_diarize: false,
                trim_start_ms: 0,
                trim_end_ms: 0,
                precise_word_timestamps: false,
            },
        )
        .unwrap();
        assert_eq!(params.model, "whisper");
        assert!(params.source_path.is_absolute());
        assert!(params.mtime_ns > 0);
    }

    #[test]
    fn build_key_params_errors_when_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("missing.wav");
        assert!(
            build_key_params(
                &p,
                KeyOptions {
                    model: "m",
                    language: "en",
                    speakers: 0,
                    no_diarize: false,
                    trim_start_ms: 0,
                    trim_end_ms: 0,
                    precise_word_timestamps: false,
                },
            )
            .is_err()
        );
    }

    #[test]
    fn key_changes_with_trim_window() {
        let mut p = KeyParams {
            source_path: PathBuf::from("/audio/a.wav"),
            mtime_ns: 1,
            model: "m".into(),
            language: "en".into(),
            speakers: 0,
            no_diarize: false,
            trim_start_ms: 0,
            trim_end_ms: 0,
            precise_word_timestamps: false,
        };
        let base = compute_key(&p);
        p.trim_start_ms = 1000;
        assert_ne!(compute_key(&p), base);
        p.trim_start_ms = 0;
        p.trim_end_ms = 5000;
        assert_ne!(compute_key(&p), base);
    }

    #[test]
    fn key_changes_with_inputs() {
        let mut p = KeyParams {
            source_path: PathBuf::from("/audio/a.wav"),
            mtime_ns: 1,
            model: "m".into(),
            language: "en".into(),
            speakers: 0,
            no_diarize: false,
            trim_start_ms: 0,
            trim_end_ms: 0,
            precise_word_timestamps: false,
        };
        let base = compute_key(&p);
        p.language = "fr".into();
        assert_ne!(compute_key(&p), base);
        p.language = "en".into();
        p.speakers = 2;
        assert_ne!(compute_key(&p), base);
        p.speakers = 0;
        p.precise_word_timestamps = true;
        assert_ne!(compute_key(&p), base);
    }
}
