mod android;
mod audio;
mod audio_toolkit;
mod browser;
mod commands;
mod config;
mod constants;
mod diarizer;
mod engine;
mod error;
mod essentials;
mod fs_utils;
#[cfg(not(target_os = "ios"))]
pub mod lang_id;
mod llm;
pub mod logfile;
mod models;
pub mod namer;
mod paths;
mod process;
mod progress;
mod runtime_install;
mod runtimes;

pub mod cuda_setup {
    pub use super::runtimes::inproc_cuda::setup;
}
mod transcriber;

pub mod api;

use tracing_subscriber::EnvFilter;

pub use android::{
    android_backup_model, android_mirror_after_install, android_notify_transcription_done,
    android_remove_from_persistent, android_reveal_path, android_share_text,
    android_start_transcription_service, android_stop_transcription_service,
};
pub use essentials::{auto_install_essentials, essential_model_ids};

fn init_logging() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "debug,wtranscriber=trace,wtranscriber_lib=trace".into()),
        )
        .init();

    logfile::info(&format!(
        "{} v{} starting",
        constants::APP_ID,
        env!("CARGO_PKG_VERSION")
    ));
}

#[cfg(target_os = "android")]
fn setup_android_paths(app: &tauri::App) {
    let data_dir = paths::android_internal_data_root().to_path_buf();
    let writable = std::fs::create_dir_all(&data_dir).is_ok()
        && std::fs::create_dir_all(data_dir.join(constants::MODELS_DIRNAME)).is_ok();
    let data_dir = if writable {
        data_dir
    } else {
        use tauri::Manager;
        let fallback = app
            .path()
            .app_local_data_dir()
            .or_else(|_| app.path().app_data_dir())
            .unwrap_or_else(|_| std::path::PathBuf::from(constants::ANDROID_SDCARD_FALLBACK));
        let _ = std::fs::create_dir_all(&fallback);
        let _ = std::fs::create_dir_all(fallback.join(constants::MODELS_DIRNAME));
        fallback
    };
    let internal_config = data_dir.join(constants::CONFIG_FILENAME);
    paths::set_config_file(internal_config.clone());
    let internal_cache = data_dir.join(constants::CACHE_DIRNAME);
    let _ = std::fs::create_dir_all(&internal_cache);
    let cache_dir = if android::android_has_all_files_access() {
        let persistent = paths::android_persistent_cache_dir().to_path_buf();
        if std::fs::create_dir_all(&persistent).is_ok() {
            android::migrate_private_cache_into(&internal_cache, &persistent);
            persistent
        } else {
            internal_cache.clone()
        }
    } else {
        internal_cache.clone()
    };
    paths::init(data_dir.clone(), data_dir.clone(), cache_dir);
    let models_dir = data_dir.join(constants::MODELS_DIRNAME);
    let _ = std::fs::create_dir_all(&models_dir);
    if android::android_has_all_files_access() {
        android::restore_config_from_persistent(&internal_config);
        if paths::android_persistent_models_dir().exists() {
            android::restore_models_from_persistent(&models_dir);
        }
        let mut cfg = config::Config::load().unwrap_or_default();
        if !cfg.use_persistent_models {
            cfg.use_persistent_models = true;
            if let Err(e) = cfg.save() {
                logfile::error(&format!("android: enabling use_persistent_models: {e}"));
            }
        }
    }
    paths::set_models_dir(models_dir);

    let persistent_workdir = paths::android_persistent_transcripts_dir().to_path_buf();
    let ext_workdir = paths::android_external_transcripts_dir().to_path_buf();
    let workdir = if android::android_has_all_files_access()
        && std::fs::create_dir_all(&persistent_workdir).is_ok()
    {
        persistent_workdir
    } else if std::fs::create_dir_all(&ext_workdir).is_ok() {
        ext_workdir
    } else {
        let fallback = data_dir.join(constants::TRANSCRIPTS_DIRNAME);
        let _ = std::fs::create_dir_all(&fallback);
        fallback
    };
    paths::set_default_workdir(workdir.clone());
    android::migrate_legacy_android_data(&data_dir, &workdir);
    android::migrate_private_transcripts_into(&data_dir, &workdir);
    logfile::info(&format!(
        "android: data={} workdir={}",
        data_dir.display(),
        workdir.display()
    ));
}

#[cfg(target_os = "android")]
fn setup_app(app: &mut tauri::App) {
    setup_android_paths(app);
}

#[cfg(not(target_os = "android"))]
const fn setup_app(_app: &mut tauri::App) {}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
#[allow(clippy::too_many_lines)]
pub fn run() {
    init_logging();

    tauri::Builder::default()
        .setup(|app| {
            setup_app(app);
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            commands::system::system_info,
            commands::config::load_config,
            commands::config::save_config,
            commands::models::list_models,
            commands::models::essential_models,
            commands::models::start_essentials,
            commands::models::install_model,
            commands::audio_files::probe_audio,
            commands::audio_files::audio_waveform,
            commands::audio_files::load_audio_meta,
            commands::audio_files::save_audio_meta,
            commands::audio_files::apply_trim,
            commands::transcribe::transcribe_file,
            commands::transcribe::redo_diarization,
            commands::transcribe::cancel_all_transcribes,
            commands::files::rename_file,
            commands::files::delete_file,
            commands::files::reveal_in_folder,
            commands::files::format_transcript,
            commands::files::share_transcript,
            commands::audio_files::probe_duration,
            commands::files::list_directory,
            commands::files::default_dir,
            commands::files::add_to_workdir,
            commands::audio_files::save_recording,
            commands::audio_files::read_audio_bytes,
            commands::diagnostics::history_load,
            commands::diagnostics::rename_speaker,
            commands::llm::suggest_filename,
            commands::diagnostics::log_tail,
            commands::diagnostics::log_clear,
            commands::diagnostics::log_renderer,
            commands::diagnostics::reset_transcript_cache,
            commands::diagnostics::reset_audio_cache,
            android::has_persistent_storage,
            android::request_persistent_storage,
            android::enable_persistent_storage,
            android::disable_persistent_storage,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            logfile::error(&format!("tauri run failed: {e}"));
            std::process::exit(1);
        });
}
