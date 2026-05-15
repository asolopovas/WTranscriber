#![allow(clippy::needless_pass_by_value)]

use serde::Serialize;

use crate::{
    audio, browser,
    error::Result,
    logfile, paths,
    transcriber::{self, Transcript},
};

#[derive(Debug, Serialize)]
pub struct ResetAppDataResult {
    pub cache_entries_removed: u64,
    pub workdir_entries_removed: u64,
}

#[tauri::command]
pub fn log_renderer(
    level: String,
    message: String,
    source: Option<String>,
    line: Option<u32>,
    column: Option<u32>,
    stack: Option<String>,
) {
    let loc = match (source.as_deref(), line, column) {
        (Some(s), Some(l), Some(c)) => format!(" at {s}:{l}:{c}"),
        (Some(s), Some(l), None) => format!(" at {s}:{l}"),
        (Some(s), None, _) => format!(" at {s}"),
        _ => String::new(),
    };
    let trace = stack
        .filter(|s| !s.is_empty())
        .map(|s| format!("\n{s}"))
        .unwrap_or_default();
    let entry = format!("[renderer/{level}] {message}{loc}{trace}");
    match level.as_str() {
        "error" | "warn" => logfile::warn(&entry),
        _ => logfile::info(&entry),
    }
}

#[tauri::command]
pub fn log_tail(max_bytes: Option<u64>) -> String {
    logfile::read_tail(max_bytes.unwrap_or(crate::constants::LOG_TAIL_DEFAULT_BYTES))
}

#[tauri::command]
pub fn log_clear() -> Result<()> {
    logfile::clear()
}

#[tauri::command]
pub fn reset_transcript_cache() -> Result<u64> {
    transcriber::cache::clear_all()
}

#[tauri::command]
pub fn reset_audio_cache() -> Result<u64> {
    audio::clear_cache()
}

#[tauri::command]
pub fn reset_app_data() -> Result<ResetAppDataResult> {
    let cache_entries_removed = clear_dir_contents(&paths::cache_dir()?)?;
    let workdir = browser::home_dir();
    let workdir_entries_removed = clear_dir_contents(&workdir)?;
    logfile::clear()?;
    Ok(ResetAppDataResult {
        cache_entries_removed,
        workdir_entries_removed,
    })
}

fn clear_dir_contents(dir: &std::path::Path) -> Result<u64> {
    std::fs::create_dir_all(dir)?;
    let mut removed = 0_u64;
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        removed = removed.saturating_add(remove_path(&path)?);
    }
    Ok(removed)
}

fn remove_path(path: &std::path::Path) -> Result<u64> {
    let meta = std::fs::symlink_metadata(path)?;
    if meta.is_dir() {
        let mut removed = 1_u64;
        for entry in std::fs::read_dir(path)? {
            removed = removed.saturating_add(remove_path(&entry?.path())?);
        }
        std::fs::remove_dir(path)?;
        Ok(removed)
    } else {
        std::fs::remove_file(path)?;
        Ok(1)
    }
}

#[tauri::command]
pub fn history_load(key: String) -> Result<Option<Transcript>> {
    transcriber::cache::load(&key)
}

#[tauri::command]
pub fn rename_speaker(key: String, old: String, new: String) -> Result<Transcript> {
    let new = new.trim().to_owned();
    if new.is_empty() {
        return Err(crate::error::Error::Config(
            "new speaker name is empty".into(),
        ));
    }
    let mut transcript = transcriber::cache::load(&key)?.ok_or_else(|| {
        crate::error::Error::Config(format!("no cached transcript for key {key}"))
    })?;
    let hits = transcript.rename_speaker(&old, &new);
    transcriber::cache::overwrite_transcript(&key, &transcript)?;
    logfile::info(&format!(
        "rename_speaker '{old}' -> '{new}' ({hits} utterances) [{key}]"
    ));
    Ok(transcript)
}
