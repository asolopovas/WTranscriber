#![allow(clippy::needless_pass_by_value)]

use serde::Serialize;
use std::path::PathBuf;

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
        _ => logfile::debug(&entry),
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
pub fn history_load(key: String, input: Option<PathBuf>) -> Result<Option<Transcript>> {
    transcriber::saved::load_for_key(&key, input.as_deref())
}

#[tauri::command]
pub fn rename_speaker(
    key: String,
    old: String,
    new: String,
    input: Option<PathBuf>,
) -> Result<Transcript> {
    let _guard = transcriber::saved::EDIT_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let new = new.trim().to_owned();
    if new.is_empty() {
        return Err(crate::error::Error::Config(
            "new speaker name is empty".into(),
        ));
    }
    let mut transcript =
        transcriber::saved::load_for_key(&key, input.as_deref())?.ok_or_else(|| {
            crate::error::Error::Config(format!("no cached transcript for key {key}"))
        })?;
    let hits = transcript.rename_speaker(&old, &new);
    persist_edit(&key, input.as_deref(), &transcript)?;
    logfile::info(&format!(
        "rename_speaker '{old}' -> '{new}' ({hits} utterances) [{key}]"
    ));
    Ok(transcript)
}

pub(super) fn persist_edit(
    key: &str,
    input: Option<&std::path::Path>,
    transcript: &Transcript,
) -> Result<()> {
    let source = transcriber::saved::source_for_key(key, input)?;
    transcriber::saved::store_edit(key, &source, transcript)?;
    sync_edit_cache(key, transcript);
    Ok(())
}

fn sync_edit_cache(key: &str, transcript: &Transcript) {
    let result = transcriber::cache::load(key).and_then(|cached| {
        if cached.is_some() {
            transcriber::cache::overwrite_transcript(key, transcript)?;
        }
        Ok(())
    });
    if let Err(error) = result {
        logfile::warn(&format!(
            "transcript edit saved; disposable cache update failed: {error}"
        ));
        if let Err(error) = transcriber::cache::invalidate(key) {
            logfile::warn(&format!("could not remove stale transcript cache: {error}"));
        }
    }
}

#[tauri::command]
pub fn set_transcript_speaker(
    key: String,
    input: PathBuf,
    index: usize,
    name: String,
) -> Result<Transcript> {
    let _guard = transcriber::saved::EDIT_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut transcript = transcriber::saved::load_for_key(&key, Some(&input))?
        .ok_or_else(|| crate::error::Error::Config("transcript was not found".into()))?;
    transcript.set_utterance_speaker(index, &name)?;
    persist_edit(&key, Some(&input), &transcript)?;
    Ok(transcript)
}

#[tauri::command]
pub fn replace_transcript_text(
    key: String,
    input: PathBuf,
    find: String,
    replacement: String,
) -> Result<Transcript> {
    let _guard = transcriber::saved::EDIT_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut transcript = transcriber::saved::load_for_key(&key, Some(&input))?
        .ok_or_else(|| crate::error::Error::Config("transcript was not found".into()))?;
    transcript.replace_text(&find, &replacement)?;
    persist_edit(&key, Some(&input), &transcript)?;
    Ok(transcript)
}

#[tauri::command]
pub fn update_transcript_text(
    key: String,
    input: PathBuf,
    index: usize,
    text: String,
) -> Result<Transcript> {
    let _guard = transcriber::saved::EDIT_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut transcript = transcriber::saved::load_for_key(&key, Some(&input))?
        .ok_or_else(|| crate::error::Error::Config("transcript was not found".into()))?;
    transcript.edit_utterance(index, &text)?;
    persist_edit(&key, Some(&input), &transcript)?;
    Ok(transcript)
}

#[tauri::command]
pub fn transcript_can_undo(key: String, input: PathBuf) -> Result<bool> {
    let _guard = transcriber::saved::EDIT_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    transcriber::saved::can_undo(&key, &input)
}

#[tauri::command]
pub fn undo_transcript_edit(key: String, input: PathBuf) -> Result<Transcript> {
    let _guard = transcriber::saved::EDIT_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let transcript = transcriber::saved::undo_edit(&key, &input)?;
    sync_edit_cache(&key, &transcript);
    Ok(transcript)
}

#[tauri::command]
pub fn delete_transcript_segment(key: String, input: PathBuf, index: usize) -> Result<Transcript> {
    let _guard = transcriber::saved::EDIT_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut transcript = transcriber::saved::load_for_key(&key, Some(&input))?
        .ok_or_else(|| crate::error::Error::Config("transcript was not found".into()))?;
    transcript.delete_utterance(index)?;
    persist_edit(&key, Some(&input), &transcript)?;
    Ok(transcript)
}

#[tauri::command]
pub fn mark_transcript_review(
    key: String,
    input: PathBuf,
    index: usize,
    marked: bool,
) -> Result<Transcript> {
    let _guard = transcriber::saved::EDIT_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut transcript = transcriber::saved::load_for_key(&key, Some(&input))?
        .ok_or_else(|| crate::error::Error::Config("transcript was not found".into()))?;
    transcript.mark_review(index, marked)?;
    persist_edit(&key, Some(&input), &transcript)?;
    Ok(transcript)
}
