use std::{path::PathBuf, sync::LazyLock};

use directories::ProjectDirs;

use crate::error::{Error, Result};

static DIRS: LazyLock<Option<ProjectDirs>> =
    LazyLock::new(|| ProjectDirs::from("com", "asolopovas", "wtranscriber"));

fn dirs() -> Result<&'static ProjectDirs> {
    DIRS.as_ref()
        .ok_or_else(|| Error::Config("cannot resolve project directories".into()))
}

pub fn config_dir() -> Result<PathBuf> {
    let d = dirs()?.config_dir().to_path_buf();
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

pub fn data_dir() -> Result<PathBuf> {
    let d = dirs()?.data_dir().to_path_buf();
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

pub fn models_dir() -> Result<PathBuf> {
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
    let d = dirs()?.cache_dir().to_path_buf();
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

pub fn config_file() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.yml"))
}
