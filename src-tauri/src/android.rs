use tauri::AppHandle;
#[cfg(target_os = "android")]
use tauri::Emitter;

use crate::config;
#[cfg(target_os = "android")]
use crate::{logfile, paths};

#[cfg(target_os = "android")]
const PERSISTENT_ROOT_DIR: &str = "/storage/emulated/0/WTranscriber";
#[cfg(target_os = "android")]
pub(crate) const PERSISTENT_MODELS_DIR: &str = "/storage/emulated/0/WTranscriber/models";
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
pub(crate) fn android_has_all_files_access() -> bool {
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
pub fn android_share_text(title: &str, text: &str) -> bool {
    android_jni::with_activity(false, |env, activity| {
        let t = env.new_string(title)?;
        let b = env.new_string(text)?;
        env.call_method(
            activity,
            android_jni::jni_str!("shareText"),
            android_jni::jni_sig!((arg0: JString, arg1: JString) -> void),
            &[(&t).into(), (&b).into()],
        )?;
        Ok(true)
    })
}

#[cfg(not(target_os = "android"))]
#[must_use]
pub const fn android_share_text(_title: &str, _text: &str) -> bool {
    false
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
pub(crate) fn restore_models_from_persistent(internal: &std::path::Path) {
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
        match copy_recursive_merge(&src, &dst) {
            Ok(b) => restored = restored.saturating_add(b),
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
fn copy_recursive_merge(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<u64> {
    let meta = std::fs::metadata(src)?;
    if meta.is_file() {
        if let Some(p) = dst.parent() {
            std::fs::create_dir_all(p)?;
        }
        if dst.exists() {
            let dst_meta = std::fs::metadata(dst)?;
            if dst_meta.is_file() && dst_meta.len() == meta.len() {
                return Ok(0);
            }
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
        total = total.saturating_add(copy_recursive_merge(&from, &dst.join(name))?);
    }
    Ok(total)
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
pub fn has_persistent_storage() -> bool {
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
pub fn request_persistent_storage() {
    #[cfg(target_os = "android")]
    {
        android_request_all_files_access();
    }
}

#[tauri::command]
#[allow(
    clippy::unnecessary_wraps,
    clippy::missing_const_for_fn,
    clippy::needless_pass_by_value
)]
pub fn enable_persistent_storage(app: AppHandle) -> std::result::Result<bool, String> {
    #[cfg(target_os = "android")]
    {
        if !android_has_all_files_access() {
            return Ok(false);
        }
        let mut cfg = config::Config::load().map_err(|e| e.to_string())?;
        cfg.use_persistent_models = true;
        cfg.save().map_err(|e| e.to_string())?;
        if let Ok(internal_config) = paths::config_file() {
            restore_config_from_persistent(&internal_config);
        }
        if let Ok(internal) = paths::models_dir() {
            if std::path::Path::new(PERSISTENT_MODELS_DIR).exists() {
                restore_models_from_persistent(&internal);
            }
            backup_models_to_persistent(&internal);
        }
        backup_config_to_persistent();
        let _ = app.emit("models:changed", true);
        return Ok(true);
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(true)
    }
}

#[cfg(target_os = "android")]
pub(crate) fn restore_config_from_persistent(internal_config: &std::path::Path) {
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

#[allow(clippy::missing_const_for_fn)] // Android branch is non-const (PathBuf, fs ops)
pub fn android_mirror_after_install() {
    #[cfg(target_os = "android")]
    {
        maybe_backup_after_install();
        backup_config_to_persistent();
    }
}

#[cfg(target_os = "android")]
pub(crate) fn maybe_backup_after_install() {
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

#[cfg(target_os = "android")]
fn backup_single_model(model_id: &str) {
    if !android_has_all_files_access() {
        return;
    }
    let Ok(internal_root) = paths::models_dir() else {
        return;
    };
    let src = internal_root.join(model_id);
    if !src.exists() {
        return;
    }
    let public_root = std::path::Path::new(PERSISTENT_MODELS_DIR);
    if std::fs::create_dir_all(public_root).is_err() {
        return;
    }
    let dst = public_root.join(model_id);
    match copy_recursive_merge(&src, &dst) {
        Ok(bytes) if bytes > 0 => logfile::info(&format!(
            "android: backed up {model_id} ({bytes} bytes) to persistent storage"
        )),
        Ok(_) => {}
        Err(e) => logfile::error(&format!(
            "android: persistent backup of {model_id} failed: {e}"
        )),
    }
}

#[allow(clippy::missing_const_for_fn)] // Android branch is non-const (PathBuf, fs ops)
pub fn android_backup_model(model_id: &str) {
    #[cfg(target_os = "android")]
    {
        backup_single_model(model_id);
        backup_config_to_persistent();
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = model_id;
    }
}

#[allow(clippy::missing_const_for_fn)] // Android branch is non-const (PathBuf, fs ops)
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
pub fn disable_persistent_storage() -> std::result::Result<(), String> {
    let mut cfg = config::Config::load().map_err(|e| e.to_string())?;
    cfg.use_persistent_models = false;
    cfg.save().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(target_os = "android")]
pub(crate) fn migrate_legacy_android_data(
    new_data_dir: &std::path::Path,
    _workdir: &std::path::Path,
) {
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
