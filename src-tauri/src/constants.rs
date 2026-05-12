use std::time::Duration;

include!("../../shared/identity.rs");

pub const CONFIG_FILENAME: &str = "config.yml";
pub const MODELS_DIRNAME: &str = "models";
pub const THIRD_PARTY_DIRNAME: &str = "third_party";

#[cfg(target_os = "android")]
pub const TRANSCRIPTS_DIRNAME: &str = "transcripts";
#[cfg(target_os = "android")]
pub const CACHE_DIRNAME: &str = "cache";

#[cfg(target_os = "android")]
pub const ANDROID_INTERNAL_DATA_ROOT: &str = "/data/user/0/com.asolopovas.wtranscriber/files";
#[cfg(target_os = "android")]
pub const ANDROID_PERSISTENT_ROOT: &str = "/storage/emulated/0/WTranscriber";
#[cfg(target_os = "android")]
pub const ANDROID_PERSISTENT_MODELS_DIR: &str = "/storage/emulated/0/WTranscriber/models";
#[cfg(target_os = "android")]
pub const ANDROID_PERSISTENT_CONFIG_FILE: &str = "/storage/emulated/0/WTranscriber/config.yml";
#[cfg(target_os = "android")]
pub const ANDROID_EXTERNAL_TRANSCRIPTS_DIR: &str =
    "/sdcard/Android/data/com.asolopovas.wtranscriber/files/transcripts";
pub const ANDROID_LEGACY_ROOT: &str = "/sdcard/Documents/WTranscriber";
pub const ANDROID_LEGACY_ROOT_EMULATED: &str = "/storage/emulated/0/Documents/WTranscriber";
#[cfg(target_os = "android")]
pub const ANDROID_SDCARD_FALLBACK: &str = "/sdcard";

pub const DEFAULT_SAMPLE_RATE: u32 = 16_000;

pub const DOWNLOAD_REQUEST_TIMEOUT: Duration = Duration::from_secs(300);
pub const DOWNLOAD_PROGRESS_INTERVAL: Duration = Duration::from_millis(250);
pub const DOWNLOAD_MAX_READ_RETRIES: u32 = 8;
pub const DOWNLOAD_MAX_DIAL_RETRIES: u32 = 30;
