#![allow(clippy::needless_pass_by_value)]

use std::path::PathBuf;

use crate::{
    audio,
    config::Config,
    error::Result,
    models::{self, ModelInfo},
    transcriber::{self, Job, Transcript},
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
    models::list()
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
