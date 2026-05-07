mod audio;
mod audio_toolkit;
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

pub mod cuda_setup {
    pub use super::runtimes::inproc_cuda::setup;
}
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
    if cfg!(target_os = "android") {
        logfile::info("runtime install skipped (android: jniLibs bundled in APK)");
        return;
    }
    let cfg = config::Config::load().unwrap_or_default();
    let variant = runtimes::SherpaVariant::from_device(cfg.device);
    install_sherpa(app, variant).await;
    if matches!(variant, runtimes::SherpaVariant::Cuda) && runtimes::cudnn_supported() {
        install_cudnn(app).await;
    }
    install_llama(app).await;
    if matches!(variant, runtimes::SherpaVariant::Cuda) {
        runtimes::inproc_cuda::setup();
        runtimes::inproc_cuda::dump_path();
    }
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



#[cfg(target_os = "android")]
fn migrate_legacy_android_data(new_data_dir: &std::path::Path, _workdir: &std::path::Path) {
    let legacy = std::path::PathBuf::from("/sdcard/Documents/WTranscriber");
    if !legacy.exists() {
        return;
    }
    let new_config = new_data_dir.join("config.yml");
    if !new_config.exists()
        && let Ok(raw) = std::fs::read_to_string(legacy.join("config.yml"))
        && std::fs::write(&new_config, &raw).is_ok()
    {
        logfile::info("android: migrated legacy config.yml");
    }
    let new_models = new_data_dir.join("models");
    let legacy_models = legacy.join("Models");
    let Ok(entries) = std::fs::read_dir(&legacy_models) else {
        return;
    };
    for e in entries.flatten() {
        let src = e.path();
        let Some(name) = src.file_name() else { continue };
        let dst = new_models.join(name);
        match copy_recursive(&src, &dst) {
            Ok(bytes) if bytes > 0 => {
                let _ = remove_recursive(&src);
                logfile::info(&format!(
                    "android: migrated {} ({bytes} bytes)",
                    name.to_string_lossy()
                ));
            }
            Ok(_) => {
                let _ = remove_recursive(&dst);
            }
            Err(e) => {
                let _ = remove_recursive(&dst);
                logfile::error(&format!(
                    "android: migrate {} failed: {e}",
                    src.display()
                ));
            }
        }
    }
}

#[cfg(target_os = "android")]
fn copy_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<u64> {
    let meta = std::fs::metadata(src)?;
    if meta.is_file() {
        if let Some(p) = dst.parent() {
            std::fs::create_dir_all(p)?;
        }
        return std::fs::copy(src, dst);
    }
    if !meta.is_dir() {
        return Ok(0);
    }
    std::fs::create_dir_all(dst)?;
    let mut total: u64 = 0;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let Some(name) = from.file_name() else { continue };
        total = total.saturating_add(copy_recursive(&from, &dst.join(name))?);
    }
    Ok(total)
}

#[cfg(target_os = "android")]
fn remove_recursive(p: &std::path::Path) -> std::io::Result<()> {
    let Ok(meta) = std::fs::metadata(p) else {
        return Ok(());
    };
    if meta.is_dir() {
        std::fs::remove_dir_all(p)
    } else {
        std::fs::remove_file(p)
    }
}

pub fn essential_model_ids() -> Vec<String> {
    [
        models::Family::Asr,
        models::Family::Diarizer,
        models::Family::Llm,
    ]
    .iter()
    .filter_map(|f| models::default_id(*f).map(String::from))
    .collect()
}

fn auto_install_essentials(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        ensure_runtimes(&app).await;
        let manager = models::manager();
        let pending: Vec<String> = essential_model_ids()
            .into_iter()
            .filter(|id| matches!(manager.status(id), Ok(models::ModelStatus::NotInstalled)))
            .collect();
        if pending.is_empty() {
            let _ = app.emit("model:essentials_done", true);
            return;
        }
        let mut handles = Vec::with_capacity(pending.len());
        for id in pending {
            let app = app.clone();
            handles.push(tauri::async_runtime::spawn(async move {
                logfile::info(&format!("auto_install {id} starting"));
                let app_for_cb = app.clone();
                let mut on_progress = move |p: models::FileProgress| {
                    let _ = app_for_cb.emit("model:progress", &p);
                };
                let manager = models::manager();
                match manager.install(&id, &mut on_progress).await {
                    Ok(()) => {
                        logfile::info(&format!("auto_install {id} ok"));
                        let _ = app.emit("model:done", &id);
                        true
                    }
                    Err(e) => {
                        logfile::error(&format!("auto_install {id}: {e}"));
                        let _ = app.emit("model:error", &id);
                        false
                    }
                }
            }));
        }
        let mut all_ok = true;
        for h in handles {
            match h.await {
                Ok(true) => {}
                _ => all_ok = false,
            }
        }
        let _ = app.emit("model:essentials_done", all_ok);
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "debug,wtranscriber=trace,wtranscriber_lib=trace".into()),
        )
        .init();

    logfile::info(&format!(
        "wtranscriber v{} starting",
        env!("CARGO_PKG_VERSION")
    ));

    tauri::Builder::default()
        .setup(|app| {
            #[cfg(target_os = "android")]
            {
                let data_dir = std::path::PathBuf::from(
                    "/data/user/0/com.asolopovas.wtranscriber/files",
                );
                let writable = std::fs::create_dir_all(&data_dir).is_ok()
                    && std::fs::create_dir_all(data_dir.join("models")).is_ok();
                let data_dir = if writable {
                    data_dir
                } else {
                    use tauri::Manager;
                    let fallback = app
                        .path()
                        .app_local_data_dir()
                        .or_else(|_| app.path().app_data_dir())
                        .unwrap_or_else(|_| std::path::PathBuf::from("/sdcard"));
                    let _ = std::fs::create_dir_all(&fallback);
                    let _ = std::fs::create_dir_all(fallback.join("models"));
                    fallback
                };
                paths::set_config_file(data_dir.join("config.yml"));
                let models_dir = data_dir.join("models");
                let _ = std::fs::create_dir_all(&models_dir);
                paths::set_models_dir(models_dir);

                let ext_workdir = std::path::PathBuf::from(
                    "/sdcard/Android/data/com.asolopovas.wtranscriber/files/transcripts",
                );
                let workdir = if std::fs::create_dir_all(&ext_workdir).is_ok() {
                    ext_workdir
                } else {
                    let fallback = data_dir.join("transcripts");
                    let _ = std::fs::create_dir_all(&fallback);
                    fallback
                };
                paths::set_default_workdir(workdir.clone());
                migrate_legacy_android_data(&data_dir, &workdir);
                logfile::info(&format!(
                    "android: data={} workdir={}",
                    data_dir.display(),
                    workdir.display()
                ));
            }
            auto_install_essentials(app.handle().clone());
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::app_version,
            commands::system_info,
            commands::load_config,
            commands::save_config,
            commands::list_models,
            commands::essential_models,
            commands::model_status,
            commands::install_model,
            commands::delete_model,
            commands::probe_audio,
            commands::audio_waveform,
            commands::load_audio_meta,
            commands::save_audio_meta,
            commands::transcribe_file,
            commands::cancel_transcribe,
            commands::rename_file,
            commands::delete_file,
            commands::export_transcript,
            commands::list_directory,
            commands::default_dir,
            commands::add_to_workdir,
            commands::save_recording,
            commands::read_audio_bytes,
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
