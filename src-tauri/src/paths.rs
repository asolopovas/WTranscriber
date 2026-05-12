use std::{
    path::PathBuf,
    sync::{LazyLock, RwLock},
};

use directories::ProjectDirs;

use crate::{
    constants,
    error::{Error, Result},
    fs_utils,
};

#[derive(Clone, Debug)]
struct Resolved {
    config: PathBuf,
    data: PathBuf,
    cache: PathBuf,
}

static OVERRIDE: RwLock<Option<Resolved>> = RwLock::new(None);

#[cfg(test)]
pub static PATHS_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
static WORKDIR_OVERRIDE: RwLock<Option<PathBuf>> = RwLock::new(None);
static MODELS_OVERRIDE: RwLock<Option<PathBuf>> = RwLock::new(None);
static CONFIG_FILE_OVERRIDE: RwLock<Option<PathBuf>> = RwLock::new(None);

static FALLBACK: LazyLock<Option<Resolved>> = LazyLock::new(|| {
    if let Some(d) = ProjectDirs::from(
        constants::APP_QUALIFIER,
        constants::APP_ORG,
        constants::APP_NAME,
    ) {
        return Some(Resolved {
            config: d.config_dir().to_path_buf(),
            data: d.data_dir().to_path_buf(),
            cache: d.cache_dir().to_path_buf(),
        });
    }
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("ANDROID_DATA").map(PathBuf::from))
        .or_else(|| std::env::temp_dir().into())?;
    Some(Resolved {
        config: base.join(".config").join(constants::APP_NAME),
        data: base.join(".local").join("share").join(constants::APP_NAME),
        cache: base.join(".cache").join(constants::APP_NAME),
    })
});

#[cfg(any(target_os = "android", test))]
pub fn init(config: PathBuf, data: PathBuf, cache: PathBuf) {
    if let Ok(mut g) = OVERRIDE.write() {
        *g = Some(Resolved {
            config,
            data,
            cache,
        });
    }
}

#[cfg(target_os = "android")]
pub fn set_default_workdir(path: PathBuf) {
    if let Ok(mut g) = WORKDIR_OVERRIDE.write() {
        *g = Some(path);
    }
}

pub fn default_workdir_override() -> Option<PathBuf> {
    WORKDIR_OVERRIDE.read().ok().and_then(|g| g.clone())
}

#[cfg(target_os = "android")]
pub fn set_models_dir(path: PathBuf) {
    if let Ok(mut g) = MODELS_OVERRIDE.write() {
        *g = Some(path);
    }
}

#[cfg(any(target_os = "android", test))]
pub fn set_config_file(path: PathBuf) {
    if let Ok(mut g) = CONFIG_FILE_OVERRIDE.write() {
        *g = Some(path);
    }
}

#[cfg(test)]
pub fn clear_test_overrides() {
    if let Ok(mut g) = OVERRIDE.write() {
        *g = None;
    }
    if let Ok(mut g) = WORKDIR_OVERRIDE.write() {
        *g = None;
    }
    if let Ok(mut g) = MODELS_OVERRIDE.write() {
        *g = None;
    }
    if let Ok(mut g) = CONFIG_FILE_OVERRIDE.write() {
        *g = None;
    }
}

fn resolved() -> Result<Resolved> {
    if let Ok(g) = OVERRIDE.read()
        && let Some(r) = g.as_ref()
    {
        return Ok(r.clone());
    }
    FALLBACK
        .as_ref()
        .cloned()
        .ok_or_else(|| Error::Config("cannot resolve project directories".into()))
}

pub fn config_dir() -> Result<PathBuf> {
    let d = resolved()?.config;
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

pub fn data_dir() -> Result<PathBuf> {
    let d = resolved()?.data;
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

pub fn models_dir() -> Result<PathBuf> {
    if let Ok(g) = MODELS_OVERRIDE.read()
        && let Some(p) = g.as_ref()
    {
        std::fs::create_dir_all(p)?;
        return Ok(p.clone());
    }
    let d = data_dir()?.join(constants::MODELS_DIRNAME);
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

pub fn third_party_dir() -> Result<PathBuf> {
    let d = data_dir()?.join(constants::THIRD_PARTY_DIRNAME);
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

pub fn cache_dir() -> Result<PathBuf> {
    let d = resolved()?.cache;
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

pub fn cache_subdir(name: &str) -> Result<PathBuf> {
    let d = cache_dir()?.join(name);
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

pub fn config_file() -> Result<PathBuf> {
    if let Ok(g) = CONFIG_FILE_OVERRIDE.read()
        && let Some(p) = g.as_ref()
    {
        fs_utils::ensure_parent_dir(p)?;
        return Ok(p.clone());
    }
    Ok(config_dir()?.join(constants::CONFIG_FILENAME))
}

#[cfg(target_os = "android")]
#[must_use]
pub fn android_persistent_root() -> &'static std::path::Path {
    std::path::Path::new(constants::ANDROID_PERSISTENT_ROOT)
}

#[cfg(target_os = "android")]
#[must_use]
pub fn android_persistent_models_dir() -> &'static std::path::Path {
    std::path::Path::new(constants::ANDROID_PERSISTENT_MODELS_DIR)
}

#[cfg(target_os = "android")]
#[must_use]
pub fn android_persistent_config_file() -> &'static std::path::Path {
    std::path::Path::new(constants::ANDROID_PERSISTENT_CONFIG_FILE)
}

#[cfg(target_os = "android")]
#[must_use]
pub fn android_external_transcripts_dir() -> &'static std::path::Path {
    std::path::Path::new(constants::ANDROID_EXTERNAL_TRANSCRIPTS_DIR)
}

#[cfg(target_os = "android")]
#[must_use]
pub fn android_internal_data_root() -> &'static std::path::Path {
    std::path::Path::new(constants::ANDROID_INTERNAL_DATA_ROOT)
}

#[cfg(target_os = "android")]
#[must_use]
pub fn android_legacy_root() -> &'static std::path::Path {
    std::path::Path::new(constants::ANDROID_LEGACY_ROOT)
}
