mod audio;
mod commands;
mod config;
mod diarizer;
mod engine;
mod error;
mod models;
mod paths;
mod transcriber;

pub mod api;

use tracing_subscriber::EnvFilter;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::app_version,
            commands::load_config,
            commands::save_config,
            commands::list_models,
            commands::model_status,
            commands::install_model,
            commands::probe_audio,
            commands::transcribe_file,
            commands::history_list,
            commands::history_load,
            commands::history_delete,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
