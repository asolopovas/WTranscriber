use std::time::Duration;

pub const APP_QUALIFIER: &str = "com";
pub const APP_ORG: &str = "asolopovas";
pub const APP_NAME: &str = "wtranscriber";
pub const APP_ID: &str = "com.asolopovas.wtranscriber";

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
pub const ANDROID_PERSISTENT_TRANSCRIPTS_DIR: &str = "/storage/emulated/0/WTranscriber/transcripts";
#[cfg(target_os = "android")]
pub const ANDROID_PERSISTENT_CACHE_DIR: &str = "/storage/emulated/0/WTranscriber/cache";
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

pub const DEFAULT_THREADS: u32 = 4;

pub const DOWNLOAD_REQUEST_TIMEOUT: Duration = Duration::from_secs(300);
pub const DOWNLOAD_PROGRESS_INTERVAL: Duration = Duration::from_millis(250);
pub const DOWNLOAD_MAX_READ_RETRIES: u32 = 8;
pub const DOWNLOAD_MAX_DIAL_RETRIES: u32 = 30;
pub const DOWNLOAD_BACKOFF_MAX: Duration = Duration::from_secs(15);

pub const LOG_FILE_MAX_BYTES: u64 = 5 * 1024 * 1024;
pub const LOG_TAIL_DEFAULT_BYTES: u64 = 256 * 1024;

pub const CHUNK_DEFAULT_SEC: f64 = 600.0;
pub const CHUNK_BOUNDARY_SEARCH_SEC: f64 = 2.0;
pub const CHUNK_BOUNDARY_WINDOW_SEC: f64 = 0.2;
pub const CHUNK_BOUNDARY_MIN_ADVANCE_SEC: f64 = 0.5;

pub const DIARIZER_CLUSTER_THRESHOLD: f32 = 0.5;
pub const DIARIZER_MIN_SPEECH_SEC: f32 = 0.2;
pub const DIARIZER_MIN_SILENCE_SEC: f32 = 0.2;

pub const LANG_ID_PROBE_SECONDS: usize = 3;
pub const LANG_ID_VAD_SCAN_SECONDS: usize = 60;
