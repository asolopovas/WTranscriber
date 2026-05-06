#![allow(clippy::needless_pass_by_value)]

use std::path::PathBuf;

use tauri::{AppHandle, Emitter};

use crate::{
    audio,
    config::Config,
    error::Result,
    models::{self, FileProgress, ModelInfo, ModelStatus},
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
    let result = models::manager().install(&id, &mut on_progress).await;
    let _ = app.emit(
        if result.is_ok() { "model:done" } else { "model:error" },
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
    let job = Job { input, config };
    transcriber::run(&job).await
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
