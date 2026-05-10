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
const PERSISTENT_ROOT_DIR: &str = "/storage/emulated/0/WTranscriber";
#[cfg(target_os = "android")]
const PERSISTENT_MODELS_DIR: &str = "/storage/emulated/0/WTranscriber/models";
#[cfg(target_os = "android")]
const PERSISTENT_CONFIG_FILE: &str = "/storage/emulated/0/WTranscriber/config.yml";

#[cfg(target_os = "android")]
#[allow(unsafe_code)]
mod android_jni {
    use std::sync::OnceLock;

    use jni::{
        Env, EnvUnowned, JavaVM,
        errors::LogErrorAndDefault,
        objects::{Global, JClass, JObject},
        sys::{JNI_VERSION_1_6, jint},
    };
    pub(super) use jni::{jni_sig, jni_str};

    static JVM: OnceLock<JavaVM> = OnceLock::new();
    static ACTIVITY: OnceLock<Global<JObject<'static>>> = OnceLock::new();

    #[unsafe(no_mangle)]
    pub extern "system" fn JNI_OnLoad(vm: *mut jni::sys::JavaVM, _: *mut std::ffi::c_void) -> jint {
        let vm = unsafe { JavaVM::from_raw(vm) };
        let _ = JVM.set(vm);
        JNI_VERSION_1_6
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_com_asolopovas_wtranscriber_MainActivity_wtSetActivity<'local>(
        mut env: EnvUnowned<'local>,
        _class: JClass<'local>,
        activity: JObject<'local>,
    ) {
        env.with_env(|env| -> jni::errors::Result<()> {
            let g = env.new_global_ref(&activity)?;
            let _ = ACTIVITY.set(g);
            Ok(())
        })
        .resolve::<LogErrorAndDefault>();
    }

    pub fn with_activity<F, R>(default: R, f: F) -> R
    where
        F: FnOnce(&mut Env, &JObject) -> jni::errors::Result<R>,
    {
        let Some(vm) = JVM.get() else { return default };
        let Some(activity) = ACTIVITY.get() else {
            return default;
        };
        match vm.attach_current_thread(|env| f(env, &activity)) {
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
        env.call_method(
            activity,
            android_jni::jni_str!("hasAllFilesAccess"),
            android_jni::jni_sig!(() -> bool),
            &[],
        )?
        .z()
    })
}

#[cfg(target_os = "android")]
fn android_request_all_files_access() {
    android_jni::with_activity((), |env, activity| {
        env.call_method(
            activity,
            android_jni::jni_str!("requestAllFilesAccess"),
            android_jni::jni_sig!(() -> void),
            &[],
        )?;
        Ok(())
    });
}

#[cfg(target_os = "android")]
pub fn android_start_transcription_service(title: &str) {
    android_jni::with_activity((), |env, activity| {
        let s = env.new_string(title)?;
        env.call_method(
            activity,
            android_jni::jni_str!("startTranscriptionService"),
            android_jni::jni_sig!((arg0: JString) -> void),
            &[(&s).into()],
        )?;
        Ok(())
    });
}

#[cfg(target_os = "android")]
pub fn android_stop_transcription_service() {
    android_jni::with_activity((), |env, activity| {
        env.call_method(
            activity,
            android_jni::jni_str!("stopTranscriptionService"),
            android_jni::jni_sig!(() -> void),
            &[],
        )?;
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
            android_jni::jni_str!("notifyTranscriptionDone"),
            android_jni::jni_sig!((arg0: JString, arg1: JString, arg2: bool) -> void),
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
        backup_config_to_persistent();
        return Ok(true);
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(true)
    }
}

#[cfg(target_os = "android")]
fn restore_config_from_persistent(internal_config: &std::path::Path) {
    if internal_config.exists() {
        return;
    }
    let public = std::path::Path::new(PERSISTENT_CONFIG_FILE);
    if !public.exists() {
        return;
    }
    if let Some(parent) = internal_config.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::copy(public, internal_config) {
        Ok(_) => logfile::info("android: restored config.yml from persistent storage"),
        Err(e) => logfile::error(&format!("android: restore config.yml failed: {e}")),
    }
}

#[cfg(target_os = "android")]
fn backup_config_to_persistent() {
    let Ok(internal) = paths::config_file() else {
        return;
    };
    if !internal.exists() {
        return;
    }
    if std::fs::create_dir_all(PERSISTENT_ROOT_DIR).is_err() {
        return;
    }
    let public = std::path::Path::new(PERSISTENT_CONFIG_FILE);
    if let Err(e) = std::fs::copy(&internal, public) {
        logfile::error(&format!("android: backup config.yml failed: {e}"));
    }
}

pub fn android_mirror_after_install() {
    #[cfg(target_os = "android")]
    {
        maybe_backup_after_install();
        backup_config_to_persistent();
    }
}

pub fn android_remove_from_persistent(model_id: &str) {
    #[cfg(target_os = "android")]
    {
        if !android_has_all_files_access() {
            return;
        }
        let public = std::path::Path::new(PERSISTENT_MODELS_DIR).join(model_id);
        if public.exists() {
            let _ = remove_recursive(&public);
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = model_id;
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
    backup_config_to_persistent();
}

static ESSENTIALS_STARTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub fn auto_install_essentials(app: tauri::AppHandle) {
    if ESSENTIALS_STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
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
        .setup(|_app| {
            #[cfg(target_os = "android")]
            let app = _app;
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
                let internal_config = data_dir.join("config.yml");
                paths::set_config_file(internal_config.clone());
                let cache_dir = data_dir.join("cache");
                let _ = std::fs::create_dir_all(&cache_dir);
                paths::init(data_dir.clone(), data_dir.clone(), cache_dir);
                let models_dir = data_dir.join("models");
                let _ = std::fs::create_dir_all(&models_dir);
                if android_has_all_files_access() {
                    restore_config_from_persistent(&internal_config);
                    if std::path::Path::new(PERSISTENT_MODELS_DIR).exists() {
                        restore_models_from_persistent(&models_dir);
                    }
                    let mut cfg = config::Config::load().unwrap_or_default();
                    if !cfg.use_persistent_models {
                        cfg.use_persistent_models = true;
                        if let Err(e) = cfg.save() {
                            logfile::error(&format!(
                                "android: enabling use_persistent_models: {e}"
                            ));
                        }
                    }
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
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            commands::system::app_version,
            commands::system::system_info,
            commands::config::load_config,
            commands::config::save_config,
            commands::models::list_models,
            commands::models::essential_models,
            commands::models::start_essentials,
            commands::models::model_status,
            commands::models::install_model,
            commands::models::delete_model,
            commands::audio_files::probe_audio,
            commands::audio_files::audio_waveform,
            commands::audio_files::load_audio_meta,
            commands::audio_files::save_audio_meta,
            commands::transcribe::transcribe_file,
            commands::transcribe::redo_diarization,
            commands::transcribe::cancel_transcribe,
            commands::files::rename_file,
            commands::files::delete_file,
            commands::files::export_transcript,
            commands::audio_files::probe_duration,
            commands::files::list_directory,
            commands::files::default_dir,
            commands::files::add_to_workdir,
            commands::audio_files::save_recording,
            commands::audio_files::read_audio_bytes,
            commands::diagnostics::history_load,
            commands::llm::suggest_filename,
            commands::diagnostics::log_path,
            commands::diagnostics::log_tail,
            commands::diagnostics::log_clear,
            commands::diagnostics::reset_transcript_cache,
            commands::diagnostics::reset_audio_cache,
            has_persistent_storage,
            request_persistent_storage,
            enable_persistent_storage,
            disable_persistent_storage,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
