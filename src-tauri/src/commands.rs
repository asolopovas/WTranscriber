#![allow(clippy::needless_pass_by_value)]

use std::path::PathBuf;

use tauri::{AppHandle, Emitter};

use crate::{
    audio,
    config::Config,
    error::Result,
    logfile,
    models::{self, FileProgress, ModelInfo, ModelStatus},
    namer::{self, Suggestion},
    transcriber::{self, CacheEntry, Job, Transcript},
};

#[tauri::command]
pub const fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[tauri::command]
pub fn load_config() -> Result<Config> {
    Config::load()
}

#[tauri::command]
pub fn save_config(config: Config) -> Result<()> {
    config.save()
}

#[tauri::command]
pub fn list_models() -> Result<Vec<ModelInfo>> {
    models::manager().list()
}

#[tauri::command]
pub fn model_status(id: String) -> Result<ModelStatus> {
    models::manager().status(&id)
}

#[tauri::command]
pub async fn install_model(app: AppHandle, id: String) -> Result<()> {
    let mut on_progress = |p: FileProgress| {
        let _ = app.emit("model:progress", &p);
    };
    logfile::info(&format!("install_model {id} starting"));
    let result = models::manager().install(&id, &mut on_progress).await;
    match &result {
        Ok(()) => logfile::info(&format!("install_model {id} ok")),
        Err(e) => logfile::error(&format!("install_model {id}: {e}")),
    }
    let _ = app.emit(
        if result.is_ok() {
            "model:done"
        } else {
            "model:error"
        },
        &id,
    );
    result
}

#[tauri::command]
pub fn probe_audio(path: PathBuf) -> Option<u64> {
    audio::probe_duration_ms(&path)
}

#[tauri::command]
pub async fn transcribe_file(input: PathBuf, config: Config) -> Result<Transcript> {
    let label = format!(
        "transcribe {} model={} engine={:?} lang={} device={:?}",
        input.display(),
        config.model,
        config.engine,
        config.language,
        config.device,
    );
    logfile::process_start(&label);
    let job = Job { input, config };
    match transcriber::run(&job).await {
        Ok(t) => {
            logfile::process_end(
                &label,
                "ok",
                &format!(
                    "{} utterances, {} ms, {} speakers",
                    t.utterances.len(),
                    t.duration_ms,
                    t.speakers_detected
                ),
            );
            Ok(t)
        }
        Err(e) => {
            logfile::error(&format!("{label}: {e}"));
            logfile::process_end(&label, "failed", &e.to_string());
            Err(e)
        }
    }
}

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
pub fn history_list() -> Vec<CacheEntry> {
    transcriber::cache_list()
}

#[tauri::command]
pub fn history_load(key: String) -> Result<Option<Transcript>> {
    transcriber::cache::load(&key)
}

#[tauri::command]
pub fn history_delete(key: String) -> Result<()> {
    transcriber::cache::invalidate(&key)
}

#[tauri::command]
pub async fn suggest_filename(transcript: Transcript) -> Result<Suggestion> {
    tokio::task::spawn_blocking(move || namer::suggest(&transcript, chrono::Local::now()))
        .await
        .map_err(|e| crate::error::Error::Transcribe(format!("task: {e}")))?
}
