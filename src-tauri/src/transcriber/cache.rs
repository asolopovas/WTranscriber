use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    error::{Error, Result},
    paths,
    transcriber::Transcript,
};

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

pub fn build_key_params(
    source_path: &Path,
    model: &str,
    language: &str,
    speakers: u32,
    no_diarize: bool,
    trim_start_ms: u64,
    trim_end_ms: u64,
) -> Result<KeyParams> {
    let abs = std::path::absolute(source_path)?;
    let meta = std::fs::metadata(&abs)?;
    let mtime_ns = meta
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    Ok(KeyParams {
        source_path: abs,
        mtime_ns,
        model: model.to_owned(),
        language: language.to_owned(),
        speakers,
        no_diarize,
        trim_start_ms,
        trim_end_ms,
    })
}

#[must_use]
pub fn compute_key(p: &KeyParams) -> String {
    let s = format!(
        "{}\0{}\0{}\0{}\0{}\0{}\0{}\0{}",
        p.source_path.display(),
        p.mtime_ns,
        p.model,
        p.language,
        p.speakers,
        p.no_diarize,
        p.trim_start_ms,
        p.trim_end_ms,
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

#[allow(dead_code)]
pub fn lookup(key: &str) -> Result<Option<(PathBuf, Entry)>> {
    let path = transcript_path(key)?;
    if !path.exists() {
        return Ok(None);
    }
    let entry = load_index()
        .into_iter()
        .find(|e| e.key == key)
        .ok_or_else(|| Error::Config("manifest missing entry".into()))?;
    Ok(Some((path, entry)))
}

pub fn load(key: &str) -> Result<Option<Transcript>> {
    let path = transcript_path(key)?;
    if !path.exists() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path)?;
    Ok(Some(serde_json::from_str(&raw)?))
}

pub fn store(mut entry: Entry, transcript: &Transcript) -> Result<PathBuf> {
    let path = transcript_path(&entry.key)?;
    let raw = serde_json::to_vec_pretty(transcript)?;
    entry.size_bytes = raw.len() as u64;
    std::fs::write(&path, &raw)?;

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
    let old_abs = std::path::absolute(old_path).unwrap_or_else(|_| old_path.to_path_buf());
    let new_abs = std::path::absolute(new_path).unwrap_or_else(|_| new_path.to_path_buf());
    let new_name = new_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut entries = load_index();
    let mut changed = false;
    for e in &mut entries {
        if e.source_path == old_abs {
            e.source_path = new_abs.clone();
            if !new_name.is_empty() {
                e.source_name = new_name.clone();
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
    let entries: Vec<Entry> = load_index().into_iter().filter(|e| e.key != key).collect();
    save_index(&entries)?;
    Ok(())
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
    use std::sync::Mutex;

    use chrono::Utc;
    use tempfile::TempDir;

    use super::*;

    static FS_LOCK: Mutex<()> = Mutex::new(());

    fn fresh_cache_root() -> TempDir {
        let tmp = tempfile::tempdir().unwrap();
        crate::paths::init(
            tmp.path().join("config"),
            tmp.path().join("data"),
            tmp.path().join("cache"),
        );
        tmp
    }

    fn sample_entry(key: &str, source_path: PathBuf) -> Entry {
        let source_name = source_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Entry {
            key: key.to_owned(),
            source_path,
            source_name,
            model: "whisper".into(),
            language: "en".into(),
            speakers: 0,
            no_diarize: false,
            utterances: 1,
            duration_ms: 1000,
            created_at: Utc::now(),
            size_bytes: 0,
        }
    }

    #[test]
    fn rename_source_updates_index_for_matching_entry() {
        let _g = FS_LOCK.lock().unwrap();
        let _tmp = fresh_cache_root();

        let dir = std::env::temp_dir().join(format!("wt-rename-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let old_path = dir.join("recording_260507-120738.wav");
        let new_path = dir.join("meeting-notes_260507-150000.wav");
        std::fs::write(&old_path, b"x").unwrap();

        let abs_old = std::path::absolute(&old_path).unwrap();
        save_index(&[sample_entry("k1", abs_old.clone())]).unwrap();

        std::fs::rename(&old_path, &new_path).unwrap();
        rename_source(&old_path, &new_path).unwrap();

        let entries = load_index();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].source_path,
            std::path::absolute(&new_path).unwrap()
        );
        assert_eq!(entries[0].source_name, "meeting-notes_260507-150000.wav");
        assert_eq!(entries[0].key, "k1");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rename_source_is_noop_when_no_entry_matches() {
        let _g = FS_LOCK.lock().unwrap();
        let _tmp = fresh_cache_root();

        let unrelated = std::env::temp_dir().join("unrelated.wav");
        save_index(&[sample_entry("k2", unrelated.clone())]).unwrap();

        let from = std::env::temp_dir().join("missing-old.wav");
        let to = std::env::temp_dir().join("missing-new.wav");
        rename_source(&from, &to).unwrap();

        let entries = load_index();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source_path, unrelated);
    }

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
        };
        let k1 = compute_key(&p);
        let k2 = compute_key(&p);
        assert_eq!(k1, k2);
        assert_eq!(k1.len(), 32);
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
        };
        let base = compute_key(&p);
        p.language = "fr".into();
        assert_ne!(compute_key(&p), base);
        p.language = "en".into();
        p.speakers = 2;
        assert_ne!(compute_key(&p), base);
    }
}
