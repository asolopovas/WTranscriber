mod audio;
mod browser;
mod commands;
mod config;
mod diarizer;
mod engine;
mod error;
mod llm;
mod logfile;
mod models;
pub mod namer;
mod paths;
mod transcriber;

pub mod api;

use tauri::Emitter;
use tracing_subscriber::EnvFilter;

fn auto_install_essentials(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let manager = models::manager();
        let entries: Vec<_> = match manager.list() {
            Ok(list) => list
                .into_iter()
                .filter(|m| m.default_active && m.status == models::ModelStatus::NotInstalled)
                .map(|m| m.id)
                .collect(),
            Err(e) => {
                logfile::error(&format!("auto_install: list failed: {e}"));
                return;
            }
        };
        for id in entries {
            logfile::info(&format!("auto_install {id} starting"));
            let app_for_cb = app.clone();
            let mut on_progress = move |p: models::FileProgress| {
                let _ = app_for_cb.emit("model:progress", &p);
            };
            let result = manager.install(&id, &mut on_progress).await;
            match &result {
                Ok(()) => {
                    logfile::info(&format!("auto_install {id} ok"));
                    let _ = app.emit("model:done", &id);
                }
                Err(e) => {
                    logfile::error(&format!("auto_install {id}: {e}"));
                    let _ = app.emit("model:error", &id);
                }
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    logfile::info(&format!(
        "wtranscriber v{} starting",
        env!("CARGO_PKG_VERSION")
    ));

    tauri::Builder::default()
        .setup(|app| {
            auto_install_essentials(app.handle().clone());
            Ok(())
        })
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
            commands::rename_file,
            commands::delete_file,
            commands::export_transcript,
            commands::list_directory,
            commands::default_dir,
            commands::history_list,
            commands::history_load,
            commands::history_delete,
            commands::suggest_filename,
            commands::log_path,
            commands::log_tail,
            commands::log_clear,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
