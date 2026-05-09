use std::path::PathBuf;

use crate::{
    error::{Error, Result},
    models::download::{Progress, download_file},
    paths,
    runtimes::extract,
};

pub const VERSION: &str = "9.21.1.3";

#[cfg(windows)]
use crate::process::quiet_command;

pub const fn target_dll() -> &'static str {
    "cudnn64_9.dll"
}

pub fn install_root() -> Option<PathBuf> {
    if !cfg!(target_os = "windows") {
        return None;
    }
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(PathBuf::from)
                .map(|p| p.join("AppData").join("Local"))
        })?;
    Some(base.join("Programs").join("cuDNN").join("v9"))
}

pub fn bin_dir() -> Option<PathBuf> {
    install_root().map(|p| p.join("bin"))
}

pub fn dll_path() -> Option<PathBuf> {
    bin_dir().map(|p| p.join(target_dll()))
}

pub fn is_installed() -> bool {
    dll_path().is_some_and(|p| p.exists())
}

pub const fn supported() -> bool {
    cfg!(target_os = "windows") && cfg!(target_arch = "x86_64")
}

fn asset_name() -> Option<String> {
    if supported() {
        Some(format!("cudnn-windows-x86_64-{VERSION}_cuda12-archive.zip"))
    } else {
        None
    }
}

fn url() -> Option<String> {
    let asset = asset_name()?;
    Some(format!(
        "https://developer.download.nvidia.com/compute/cudnn/redist/cudnn/windows-x86_64/{asset}"
    ))
}

pub async fn ensure(on_progress: &mut (dyn FnMut(Progress) + Send)) -> Result<PathBuf> {
    let target_root = install_root()
        .ok_or_else(|| Error::Config("cuDNN auto-install only supports Windows x86_64".into()))?;
    let target_bin = target_root.join("bin");
    let target_dll_path = target_bin.join(target_dll());
    if target_dll_path.exists() {
        return Ok(target_dll_path);
    }

    let url = url().ok_or_else(|| Error::Config("cuDNN url unavailable".into()))?;
    let asset = asset_name().unwrap();

    let cache = paths::cache_dir()?.join("cudnn");
    std::fs::create_dir_all(&cache)?;
    let archive = cache.join(&asset);

    if !archive.exists() {
        download_file(&archive, &url, None, on_progress).await?;
    }

    let staging = cache.join(format!("staging-{VERSION}"));
    if staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    std::fs::create_dir_all(&staging)?;

    extract(&archive, &staging)?;

    let src_root = locate_archive_root(&staging).ok_or_else(|| {
        Error::Config(format!(
            "cuDNN archive layout unexpected (no bin/{} in {})",
            target_dll(),
            staging.display()
        ))
    })?;

    if target_root.exists() {
        let _ = std::fs::remove_dir_all(&target_root);
    }
    if let Some(parent) = target_root.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::rename(&src_root, &target_root).is_err() {
        crate::runtimes::copy_dir_all(&src_root, &target_root)?;
    }

    flatten_x64(&target_root.join("bin"));
    flatten_x64(&target_root.join("lib"));

    let _ = std::fs::remove_dir_all(&staging);

    if !target_dll_path.exists() {
        return Err(Error::Config(format!(
            "cuDNN install incomplete: {} missing",
            target_dll_path.display()
        )));
    }
    Ok(target_dll_path)
}

pub fn ensure_on_path() {
    let Some(bin) = bin_dir() else { return };
    if !bin.join(target_dll()).exists() {
        return;
    }
    if cfg!(windows) {
        match persist_user_path(&bin) {
            Ok(true) => {
                crate::logfile::info(&format!("cuDNN added to user PATH: {}", bin.display()));
            }
            Ok(false) => {}
            Err(e) => crate::logfile::warn(&format!("cuDNN PATH persist failed: {e}")),
        }
    }
}

pub fn augmented_path() -> Option<std::ffi::OsString> {
    let bin = bin_dir()?;
    if !bin.join(target_dll()).exists() {
        return None;
    }
    let current = std::env::var_os("PATH").unwrap_or_default();
    let sep = if cfg!(windows) { ";" } else { ":" };
    let current_str = current.to_string_lossy();
    let bin_canon = bin.canonicalize().ok();
    let already = current_str.split(sep).any(|p| {
        std::path::Path::new(p)
            .canonicalize()
            .ok()
            .zip(bin_canon.as_ref())
            .is_some_and(|(c, b)| &c == b)
    });
    if already {
        return Some(current);
    }
    let mut new_path = std::ffi::OsString::from(bin.as_os_str());
    new_path.push(sep);
    new_path.push(&current);
    Some(new_path)
}

#[cfg(windows)]
fn persist_user_path(bin: &std::path::Path) -> Result<bool> {
    let bin_str = bin.to_string_lossy().to_string();
    let current = read_user_path().unwrap_or_default();
    if current.split(';').any(|p| p.eq_ignore_ascii_case(&bin_str)) {
        return Ok(false);
    }
    let new_value = if current.is_empty() {
        bin_str
    } else if current.ends_with(';') {
        format!("{current}{bin_str}")
    } else {
        format!("{current};{bin_str}")
    };
    let status = quiet_command("reg")
        .args([
            "add",
            "HKCU\\Environment",
            "/v",
            "Path",
            "/t",
            "REG_EXPAND_SZ",
            "/d",
            &new_value,
            "/f",
        ])
        .status()
        .map_err(|e| Error::Config(format!("reg add failed: {e}")))?;
    if !status.success() {
        return Err(Error::Config(format!("reg add exit {status}")));
    }
    broadcast_settings_change();
    Ok(true)
}

#[cfg(not(windows))]
#[allow(clippy::missing_const_for_fn, clippy::unnecessary_wraps)]
fn persist_user_path(_bin: &std::path::Path) -> Result<bool> {
    Ok(false)
}

#[cfg(windows)]
fn read_user_path() -> Option<String> {
    let out = quiet_command("reg")
        .args(["query", "HKCU\\Environment", "/v", "Path"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("Path") {
            continue;
        }
        let rest = trimmed.trim_start_matches("Path").trim_start();
        let after_type = rest
            .split_once("REG_EXPAND_SZ")
            .or_else(|| rest.split_once("REG_SZ"))
            .map(|(_, v)| v.trim().to_string());
        if let Some(v) = after_type {
            return Some(v);
        }
    }
    None
}

#[cfg(windows)]
fn broadcast_settings_change() {
    let _ = quiet_command("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-Command",
            "Add-Type -Namespace Win32 -Name NativeMethods -MemberDefinition '[DllImport(\"user32.dll\", SetLastError=true)]public static extern System.IntPtr SendMessageTimeout(System.IntPtr hWnd, uint Msg, System.UIntPtr wParam, string lParam, uint fuFlags, uint uTimeout, out System.UIntPtr lpdwResult);'; $r=[System.UIntPtr]::Zero; [void][Win32.NativeMethods]::SendMessageTimeout([System.IntPtr]0xffff, 0x1A, [System.UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$r)",
        ])
        .status();
}

fn locate_archive_root(root: &std::path::Path) -> Option<PathBuf> {
    crate::process::walk_for_file(root, 5, |p| {
        if !p.is_dir() {
            return false;
        }
        let bin = p.join("bin");
        bin.join(target_dll()).exists() || bin.join("x64").join(target_dll()).exists()
    })
}

fn flatten_x64(dir: &std::path::Path) {
    let nested = dir.join("x64");
    if !nested.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(&nested) else {
        return;
    };
    for e in entries.flatten() {
        let from = e.path();
        let Some(name) = from.file_name() else {
            continue;
        };
        let to = dir.join(name);
        if to.exists() {
            continue;
        }
        if std::fs::rename(&from, &to).is_err() {
            if from.is_dir() {
                let _ = crate::runtimes::copy_dir_all(&from, &to);
            } else {
                let _ = std::fs::copy(&from, &to);
            }
        }
    }
    let _ = std::fs::remove_dir_all(&nested);
}
