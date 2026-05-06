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
mod progress;
mod runtimes;
mod transcriber;

pub mod api;

use serde::Serialize;
use tauri::Emitter;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone, Serialize)]
struct RuntimeProgress {
    id: String,
    downloaded: u64,
    total: u64,
}

async fn ensure_runtimes(app: &tauri::AppHandle) {
    let cfg = config::Config::load().unwrap_or_default();
    let variant = runtimes::SherpaVariant::from_device(cfg.device);
    install_sherpa(app, variant).await;
    if matches!(variant, runtimes::SherpaVariant::Cuda) && runtimes::cudnn_supported() {
        install_cudnn(app).await;
    }
    install_llama(app).await;
}

async fn install_cudnn(app: &tauri::AppHandle) {
    let id = "cudnn".to_string();
    if runtimes::cudnn_installed() {
        logfile::info(&format!("runtime {id} already installed"));
    } else {
        logfile::info(&format!(
            "runtime install {id} starting (~700 MB, one-time)"
        ));
        let mut on_progress = progress_emitter(app, id.clone());
        match runtimes::ensure_cudnn(&mut on_progress).await {
            Ok(dll) => {
                logfile::info(&format!("runtime install {id} ok ({})", dll.display()));
                let _ = app.emit("runtime:done", &id);
            }
            Err(e) => {
                logfile::error(&format!("runtime install {id}: {e}"));
                let _ = app.emit("runtime:error", &id);
                return;
            }
        }
    }
    runtimes::cudnn::ensure_on_path();
}

async fn install_sherpa(app: &tauri::AppHandle, variant: runtimes::SherpaVariant) {
    let id = format!("sherpa-onnx-{}", variant.slug());
    if runtimes::sherpa_installed(variant) {
        logfile::info(&format!("runtime {id} already installed"));
        return;
    }
    logfile::info(&format!("runtime install {id} starting"));
    let mut on_progress = progress_emitter(app, id.clone());
    match runtimes::ensure_sherpa(variant, &mut on_progress).await {
        Ok(dir) => {
            logfile::info(&format!("runtime install {id} ok ({})", dir.display()));
            let _ = app.emit("runtime:done", &id);
        }
        Err(e) => {
            logfile::error(&format!("runtime install {id}: {e}"));
            let _ = app.emit("runtime:error", &id);
        }
    }
}

async fn install_llama(app: &tauri::AppHandle) {
    let id = "llama.cpp".to_string();
    if runtimes::llama_installed() {
        logfile::info(&format!("runtime {id} already installed"));
        return;
    }
    logfile::info(&format!("runtime install {id} starting"));
    let mut on_progress = progress_emitter(app, id.clone());
    match runtimes::ensure_llama(&mut on_progress).await {
        Ok(dir) => {
            logfile::info(&format!("runtime install {id} ok ({})", dir.display()));
            let _ = app.emit("runtime:done", &id);
        }
        Err(e) => {
            logfile::error(&format!("runtime install {id}: {e}"));
            let _ = app.emit("runtime:error", &id);
        }
    }
}

fn progress_emitter(
    app: &tauri::AppHandle,
    id: String,
) -> impl FnMut(models::download::Progress) + Send + use<> {
    let app = app.clone();
    move |p: models::download::Progress| {
        let _ = app.emit(
            "runtime:progress",
            &RuntimeProgress {
                id: id.clone(),
                downloaded: p.downloaded,
                total: p.total,
            },
        );
    }
}

fn auto_install_essentials(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        ensure_runtimes(&app).await;
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
            commands::audio_waveform,
            commands::transcribe_file,
            commands::cancel_transcribe,
            commands::rename_file,
            commands::delete_file,
            commands::export_transcript,
            commands::list_directory,
            commands::default_dir,
            commands::add_to_workdir,
            commands::history_load,
            commands::suggest_filename,
            commands::log_path,
            commands::log_tail,
            commands::log_clear,
            commands::reset_transcript_cache,
            commands::reset_audio_cache,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
