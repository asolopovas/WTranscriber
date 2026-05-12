use std::path::PathBuf;

use crate::{
    error::{Error, Result},
    models::download::{Progress, download_file},
    paths,
    runtimes::{extract, locate_bin_dir, move_or_copy_dir},
};

pub const BUILD: &str = "b9045";

pub fn root_dir() -> Result<PathBuf> {
    let d = paths::third_party_dir()?.join("llama.cpp");
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

pub fn install_dir() -> Result<PathBuf> {
    Ok(root_dir()?.join(BUILD))
}

pub fn bin_dir() -> Result<PathBuf> {
    Ok(install_dir()?.join("bin"))
}

pub const fn binary_name() -> &'static str {
    if cfg!(windows) {
        "llama-cli.exe"
    } else {
        "llama-cli"
    }
}

pub fn binary_path() -> Result<PathBuf> {
    Ok(bin_dir()?.join(binary_name()))
}

pub fn is_installed() -> bool {
    binary_path().is_ok_and(|p| p.exists())
}

pub fn find() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    if let Some(p) = find_android() {
        return Some(p);
    }
    let p = binary_path().ok()?;
    p.exists().then_some(p)
}

#[cfg(target_os = "android")]
fn find_android() -> Option<PathBuf> {
    let needle = "libllama-cli.so";
    if let Ok(maps) = std::fs::read_to_string("/proc/self/maps") {
        for line in maps.lines() {
            if let Some(idx) = line.find("/data/app/") {
                let path = &line[idx..];
                if path.ends_with(".so")
                    && let Some(parent) = std::path::Path::new(path).parent()
                {
                    let candidate = parent.join(needle);
                    if candidate.exists() {
                        return Some(candidate);
                    }
                }
            }
        }
    }
    for env in ["LD_LIBRARY_PATH", "ANDROID_NATIVE_LIBS_DIR"] {
        if let Ok(v) = std::env::var(env) {
            for p in v.split(':') {
                if p.is_empty() {
                    continue;
                }
                let candidate = std::path::Path::new(p).join(needle);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

fn asset_name() -> Option<String> {
    let b = BUILD;
    let name = if cfg!(target_os = "windows") && cfg!(target_arch = "x86_64") {
        format!("llama-{b}-bin-win-cpu-x64.zip")
    } else if cfg!(target_os = "windows") && cfg!(target_arch = "aarch64") {
        format!("llama-{b}-bin-win-cpu-arm64.zip")
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        format!("llama-{b}-bin-ubuntu-x64.tar.gz")
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "aarch64") {
        format!("llama-{b}-bin-ubuntu-arm64.tar.gz")
    } else {
        return None;
    };
    Some(name)
}

fn url() -> Option<String> {
    let asset = asset_name()?;
    Some(format!(
        "https://github.com/ggml-org/llama.cpp/releases/download/{BUILD}/{asset}"
    ))
}

pub async fn ensure(on_progress: &mut (dyn FnMut(Progress) + Send)) -> Result<PathBuf> {
    let dir = install_dir()?;
    if is_installed() {
        return Ok(dir);
    }
    let url = url().ok_or_else(|| Error::Config("no llama.cpp asset for this platform".into()))?;
    let asset = asset_name().expect("asset_name is Some when url() is Some");

    let cache = paths::cache_subdir("llama.cpp")?;
    let archive = cache.join(&asset);

    if !archive.exists() {
        download_file(&archive, &url, None, on_progress).await?;
    }

    let staging = cache.join("staging");
    if staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    std::fs::create_dir_all(&staging)?;

    extract(&archive, &staging)?;

    let bin_src = locate_bin_dir(&staging, binary_name()).ok_or_else(|| {
        Error::Config(format!(
            "llama.cpp archive layout unexpected (no {} found in {})",
            binary_name(),
            staging.display()
        ))
    })?;

    let target_bin = bin_dir()?;
    move_or_copy_dir(&bin_src, &target_bin)?;

    let _ = std::fs::remove_dir_all(&staging);
    Ok(dir)
}
