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
mod process;
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
    #[serde(flatten)]
    progress: models::download::ByteProgress,
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
                progress: p,
            },
        );
    }
}

#[cfg(target_os = "android")]
const PERSISTENT_MODELS_DIR: &str = "/storage/emulated/0/WTranscriber/models";

#[cfg(target_os = "android")]
#[allow(unsafe_code)]
mod android_jni {
    use std::sync::OnceLock;

    use jni::{
        JNIEnv, JavaVM,
        objects::{GlobalRef, JClass, JObject},
        sys::{JNI_VERSION_1_6, jint},
    };

    static JVM: OnceLock<JavaVM> = OnceLock::new();
    static ACTIVITY: OnceLock<GlobalRef> = OnceLock::new();

    #[unsafe(no_mangle)]
    pub extern "system" fn JNI_OnLoad(vm: *mut jni::sys::JavaVM, _: *mut std::ffi::c_void) -> jint {
        if let Ok(vm) = unsafe { JavaVM::from_raw(vm) } {
            let _ = JVM.set(vm);
        }
        JNI_VERSION_1_6
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_asolopovas_wtranscriber_MainActivity_wtSetActivity(
        env: JNIEnv,
        _class: JClass,
        activity: JObject,
    ) {
        if let Ok(g) = env.new_global_ref(&activity) {
            let _ = ACTIVITY.set(g);
        }
    }

    pub fn with_activity<F, R>(default: R, f: F) -> R
    where
        F: FnOnce(&mut JNIEnv, &JObject) -> jni::errors::Result<R>,
    {
        let Some(vm) = JVM.get() else { return default };
        let Some(activity) = ACTIVITY.get() else {
            return default;
        };
        let Ok(mut env) = vm.attach_current_thread() else {
            return default;
        };
        match f(&mut env, activity.as_obj()) {
            Ok(v) => v,
            Err(e) => {
                crate::logfile::error(&format!("jni call: {e}"));
                default
            }
        }
    }
}

#[cfg(target_os = "android")]
fn android_has_all_files_access() -> bool {
    android_jni::with_activity(false, |env, activity| {
        env.call_method(activity, "hasAllFilesAccess", "()Z", &[])?
            .z()
    })
}

#[cfg(target_os = "android")]
fn android_request_all_files_access() {
    android_jni::with_activity((), |env, activity| {
        env.call_method(activity, "requestAllFilesAccess", "()V", &[])?;
        Ok(())
    });
}

#[cfg(target_os = "android")]
pub fn android_start_transcription_service(title: &str) {
    android_jni::with_activity((), |env, activity| {
        let s = env.new_string(title)?;
        env.call_method(
            activity,
            "startTranscriptionService",
            "(Ljava/lang/String;)V",
            &[(&s).into()],
        )?;
        Ok(())
    });
}

#[cfg(target_os = "android")]
pub fn android_stop_transcription_service() {
    android_jni::with_activity((), |env, activity| {
        env.call_method(activity, "stopTranscriptionService", "()V", &[])?;
        Ok(())
    });
}

#[cfg(target_os = "android")]
pub fn android_notify_transcription_done(title: &str, text: &str, success: bool) {
    android_jni::with_activity((), |env, activity| {
        let t = env.new_string(title)?;
        let b = env.new_string(text)?;
        env.call_method(
            activity,
            "notifyTranscriptionDone",
            "(Ljava/lang/String;Ljava/lang/String;Z)V",
            &[(&t).into(), (&b).into(), success.into()],
        )?;
        Ok(())
    });
}

#[cfg(not(target_os = "android"))]
pub const fn android_start_transcription_service(_title: &str) {}

#[cfg(not(target_os = "android"))]
pub const fn android_stop_transcription_service() {}

#[cfg(not(target_os = "android"))]
pub const fn android_notify_transcription_done(_title: &str, _text: &str, _success: bool) {}

#[cfg(target_os = "android")]
fn restore_models_from_persistent(internal: &std::path::Path) {
    let public = std::path::Path::new(PERSISTENT_MODELS_DIR);
    if !public.exists() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(public) else {
        return;
    };
    let mut restored: u64 = 0;
    for e in entries.flatten() {
        let src = e.path();
        let Some(name) = src.file_name() else {
            continue;
        };
        let dst = internal.join(name);
        if dst.exists() {
            continue;
        }
        match copy_recursive(&src, &dst) {
            Ok(b) if b > 0 => restored = restored.saturating_add(b),
            Ok(_) => {
                let _ = remove_recursive(&dst);
            }
            Err(e) => logfile::error(&format!(
                "android: persistent restore {} failed: {e}",
                src.display()
            )),
        }
    }
    if restored > 0 {
        logfile::info(&format!(
            "android: restored {restored} bytes from persistent storage"
        ));
    }
}

#[cfg(target_os = "android")]
fn backup_models_to_persistent(internal: &std::path::Path) {
    let public = std::path::Path::new(PERSISTENT_MODELS_DIR);
    if std::fs::create_dir_all(public).is_err() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(internal) else {
        return;
    };
    let mut backed: u64 = 0;
    for e in entries.flatten() {
        let src = e.path();
        let Some(name) = src.file_name() else {
            continue;
        };
        let dst = public.join(name);
        if dst.exists() {
            continue;
        }
        match copy_recursive(&src, &dst) {
            Ok(b) if b > 0 => backed = backed.saturating_add(b),
            Ok(_) => {
                let _ = remove_recursive(&dst);
            }
            Err(e) => logfile::error(&format!(
                "android: persistent backup {} failed: {e}",
                src.display()
            )),
        }
    }
    if backed > 0 {
        logfile::info(&format!(
            "android: backed up {backed} bytes to persistent storage"
        ));
    }
}

#[tauri::command]
#[allow(clippy::missing_const_for_fn)]
fn has_persistent_storage() -> bool {
    #[cfg(target_os = "android")]
    {
        return android_has_all_files_access();
    }
    #[cfg(not(target_os = "android"))]
    {
        true
    }
}

#[tauri::command]
#[allow(clippy::missing_const_for_fn)]
fn request_persistent_storage() {
    #[cfg(target_os = "android")]
    {
        android_request_all_files_access();
    }
}

#[tauri::command]
#[allow(clippy::unnecessary_wraps, clippy::missing_const_for_fn)]
fn enable_persistent_storage() -> std::result::Result<bool, String> {
    #[cfg(target_os = "android")]
    {
        if !android_has_all_files_access() {
            return Ok(false);
        }
        let mut cfg = config::Config::load().map_err(|e| e.to_string())?;
        cfg.use_persistent_models = true;
        cfg.save().map_err(|e| e.to_string())?;
        if let Ok(internal) = paths::models_dir() {
            backup_models_to_persistent(&internal);
        }
        return Ok(true);
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(true)
    }
}

#[tauri::command]
fn disable_persistent_storage() -> std::result::Result<(), String> {
    let mut cfg = config::Config::load().map_err(|e| e.to_string())?;
    cfg.use_persistent_models = false;
    cfg.save().map_err(|e| e.to_string())?;
    Ok(())
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
        let Some(name) = src.file_name() else {
            continue;
        };
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
                logfile::error(&format!("android: migrate {} failed: {e}", src.display()));
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
        let Some(name) = from.file_name() else {
            continue;
        };
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

#[must_use]
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

#[cfg(target_os = "android")]
fn maybe_backup_after_install() {
    let cfg = config::Config::load().unwrap_or_default();
    if !cfg.use_persistent_models {
        return;
    }
    if !android_has_all_files_access() {
        return;
    }
    if let Ok(internal) = paths::models_dir() {
        backup_models_to_persistent(&internal);
    }
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
        #[cfg(target_os = "android")]
        maybe_backup_after_install();
        let _ = app.emit("model:essentials_done", all_ok);
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
#[allow(clippy::too_many_lines)]
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
                let data_dir =
                    std::path::PathBuf::from("/data/user/0/com.asolopovas.wtranscriber/files");
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
                let cache_dir = data_dir.join("cache");
                let _ = std::fs::create_dir_all(&cache_dir);
                paths::init(data_dir.clone(), data_dir.clone(), cache_dir);
                let models_dir = data_dir.join("models");
                let _ = std::fs::create_dir_all(&models_dir);
                let cfg = config::Config::load().unwrap_or_default();
                if cfg.use_persistent_models && android_has_all_files_access() {
                    restore_models_from_persistent(&models_dir);
                }
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
            has_persistent_storage,
            request_persistent_storage,
            enable_persistent_storage,
            disable_persistent_storage,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
