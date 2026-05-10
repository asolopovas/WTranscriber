#![allow(clippy::needless_pass_by_value)]

use crate::{
    audio,
    error::Result,
    logfile,
    transcriber::{self, Transcript},
};

#[tauri::command]
pub fn log_path() -> Result<String> {
    Ok(logfile::log_path()?.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn log_tail(max_bytes: Option<u64>) -> String {
    logfile::read_tail(max_bytes.unwrap_or(256 * 1024))
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
