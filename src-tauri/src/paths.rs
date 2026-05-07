use std::{
    path::PathBuf,
    sync::{LazyLock, RwLock},
};

use directories::ProjectDirs;

use crate::error::{Error, Result};

#[derive(Clone, Debug)]
struct Resolved {
    config: PathBuf,
    data: PathBuf,
    cache: PathBuf,
}

static OVERRIDE: RwLock<Option<Resolved>> = RwLock::new(None);
static WORKDIR_OVERRIDE: RwLock<Option<PathBuf>> = RwLock::new(None);
static MODELS_OVERRIDE: RwLock<Option<PathBuf>> = RwLock::new(None);
static CONFIG_FILE_OVERRIDE: RwLock<Option<PathBuf>> = RwLock::new(None);

static FALLBACK: LazyLock<Option<Resolved>> = LazyLock::new(|| {
    if let Some(d) = ProjectDirs::from("com", "asolopovas", "wtranscriber") {
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
        config: base.join(".config").join("wtranscriber"),
        data: base.join(".local").join("share").join("wtranscriber"),
        cache: base.join(".cache").join("wtranscriber"),
    })
});

pub fn init(config: PathBuf, data: PathBuf, cache: PathBuf) {
    if let Ok(mut g) = OVERRIDE.write() {
        *g = Some(Resolved {
            config,
            data,
            cache,
        });
    }
}

pub fn set_default_workdir(path: PathBuf) {
    if let Ok(mut g) = WORKDIR_OVERRIDE.write() {
        *g = Some(path);
    }
}

pub fn default_workdir_override() -> Option<PathBuf> {
    WORKDIR_OVERRIDE.read().ok().and_then(|g| g.clone())
}

pub fn set_models_dir(path: PathBuf) {
    if let Ok(mut g) = MODELS_OVERRIDE.write() {
        *g = Some(path);
    }
}

pub fn set_config_file(path: PathBuf) {
    if let Ok(mut g) = CONFIG_FILE_OVERRIDE.write() {
        *g = Some(path);
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
    let d = data_dir()?.join("models");
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

pub fn third_party_dir() -> Result<PathBuf> {
    let d = data_dir()?.join("third_party");
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

pub fn cache_dir() -> Result<PathBuf> {
    let d = resolved()?.cache;
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

pub fn config_file() -> Result<PathBuf> {
    if let Ok(g) = CONFIG_FILE_OVERRIDE.read()
        && let Some(p) = g.as_ref()
    {
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        return Ok(p.clone());
    }
    Ok(config_dir()?.join("config.yml"))
}
