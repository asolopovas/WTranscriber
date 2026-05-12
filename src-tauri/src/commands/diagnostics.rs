#![allow(clippy::needless_pass_by_value)]

use crate::{
    audio,
    error::Result,
    logfile,
    transcriber::{self, Transcript},
};

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
