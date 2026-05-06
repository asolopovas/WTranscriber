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
    })
}

#[must_use]
pub fn compute_key(p: &KeyParams) -> String {
    let s = format!(
        "{}\0{}\0{}\0{}\0{}\0{}",
        p.source_path.display(),
        p.mtime_ns,
        p.model,
        p.language,
        p.speakers,
        p.no_diarize
    );
    let hash = Sha256::digest(s.as_bytes());
    hex::encode(&hash[..16])
}

fn load_index() -> Vec<Entry> {
    let Ok(path) = index_path() else { return Vec::new() };
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

pub fn invalidate(key: &str) -> Result<()> {
    let path = transcript_path(key)?;
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    let entries: Vec<Entry> = load_index()
        .into_iter()
        .filter(|e| e.key != key)
        .collect();
    save_index(&entries)?;
    Ok(())
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
        };
        let base = compute_key(&p);
        p.language = "fr".into();
        assert_ne!(compute_key(&p), base);
        p.language = "en".into();
        p.speakers = 2;
        assert_ne!(compute_key(&p), base);
    }
}
