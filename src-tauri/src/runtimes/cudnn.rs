use std::path::PathBuf;

use crate::{
    error::{Error, Result},
    fs_utils,
    models::download::{Progress, download_file},
    paths,
    runtimes::extract,
};

pub const VERSION: &str = "9.21.1.3";

#[cfg(windows)]
use crate::process::quiet_command;

pub const fn target_library() -> &'static str {
    if cfg!(windows) {
        "cudnn64_9.dll"
    } else {
        "libcudnn.so.9"
    }
}

pub const fn target_dll() -> &'static str {
    target_library()
}

pub fn install_root() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("USERPROFILE")
                    .map(PathBuf::from)
                    .map(|p| p.join("AppData").join("Local"))
            })?;
        return Some(base.join("Programs").join("cuDNN").join("v9"));
    }
    if cfg!(target_os = "linux") {
        return paths::third_party_dir()
            .ok()
            .map(|p| p.join("cudnn").join("v9"));
    }
    None
}

pub fn library_dir() -> Option<PathBuf> {
    let root = install_root()?;
    Some(if cfg!(windows) {
        root.join("bin")
    } else {
        root.join("lib")
    })
}

pub fn bin_dir() -> Option<PathBuf> {
    library_dir()
}

pub fn library_path() -> Option<PathBuf> {
    library_dir().map(|p| p.join(target_library()))
}

pub fn is_installed() -> bool {
    library_path().is_some_and(|p| p.exists())
}

pub const fn supported() -> bool {
    (cfg!(target_os = "windows") || cfg!(target_os = "linux")) && cfg!(target_arch = "x86_64")
}

fn asset_name() -> Option<String> {
    if cfg!(target_os = "windows") && cfg!(target_arch = "x86_64") {
        Some(format!("cudnn-windows-x86_64-{VERSION}_cuda12-archive.zip"))
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        Some(format!(
            "cudnn-linux-x86_64-{VERSION}_cuda12-archive.tar.xz"
        ))
    } else {
        None
    }
}

fn url() -> Option<String> {
    let asset = asset_name()?;
    let subpath = if cfg!(target_os = "windows") {
        "windows-x86_64"
    } else {
        "linux-x86_64"
    };
    Some(format!(
        "https://developer.download.nvidia.com/compute/cudnn/redist/cudnn/{subpath}/{asset}"
    ))
}

pub const fn library_search_env() -> &'static str {
    if cfg!(windows) {
        "PATH"
    } else {
        "LD_LIBRARY_PATH"
    }
}

pub async fn ensure(on_progress: &mut (dyn FnMut(Progress) + Send)) -> Result<PathBuf> {
    let target_root = install_root()
        .ok_or_else(|| Error::Config("cuDNN auto-install unsupported on this platform".into()))?;
    let target_lib_dir = if cfg!(windows) {
        target_root.join("bin")
    } else {
        target_root.join("lib")
    };
    let target_lib_path = target_lib_dir.join(target_library());
    if target_lib_path.exists() {
        return Ok(target_lib_path);
    }

    let url = url().ok_or_else(|| Error::Config("cuDNN url unavailable".into()))?;
    let asset = asset_name().expect("asset_name is Some when url() is Some");

    let cache = paths::cache_subdir("cudnn")?;
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
            "cuDNN archive layout unexpected (no {} in {})",
            target_library(),
            staging.display()
        ))
    })?;

    let _ = std::fs::remove_dir_all(&target_root);
    fs_utils::ensure_parent_dir(&target_root)?;
    if std::fs::rename(&src_root, &target_root).is_err() {
        crate::runtimes::copy_dir_all(&src_root, &target_root)?;
    }

    flatten_x64(&target_root.join("bin"));
    flatten_x64(&target_root.join("lib"));

    let _ = std::fs::remove_dir_all(&staging);

    if !target_lib_path.exists() {
        return Err(Error::Config(format!(
            "cuDNN install incomplete: {} missing",
            target_lib_path.display()
        )));
    }
    Ok(target_lib_path)
}

pub fn ensure_on_path() {
    let Some(dir) = library_dir() else { return };
    if !dir.join(target_library()).exists() {
        return;
    }
    if cfg!(windows) {
        match persist_user_path(&dir) {
            Ok(true) => {
                crate::logfile::info(&format!("cuDNN added to user PATH: {}", dir.display()));
            }
            Ok(false) => {}
            Err(e) => crate::logfile::warn(&format!("cuDNN PATH persist failed: {e}")),
        }
    }
}

pub fn augmented_library_path() -> Option<(&'static str, std::ffi::OsString)> {
    let dir = library_dir()?;
    if !dir.join(target_library()).exists() {
        return None;
    }
    let env_name = library_search_env();
    let current = std::env::var_os(env_name).unwrap_or_default();
    let sep = if cfg!(windows) { ";" } else { ":" };
    let current_str = current.to_string_lossy();
    let dir_canon = dir.canonicalize().ok();
    let already = !current_str.is_empty()
        && current_str.split(sep).any(|p| {
            std::path::Path::new(p)
                .canonicalize()
                .ok()
                .zip(dir_canon.as_ref())
                .is_some_and(|(c, b)| &c == b)
        });
    if already {
        return Some((env_name, current));
    }
    let mut new_path = std::ffi::OsString::from(dir.as_os_str());
    if !current.is_empty() {
        new_path.push(sep);
        new_path.push(&current);
    }
    Some((env_name, new_path))
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
    let lib = target_library();
    crate::process::walk_for_file(root, 5, |p| {
        if !p.is_dir() {
            return false;
        }
        if cfg!(windows) {
            let bin = p.join("bin");
            bin.join(lib).exists() || bin.join("x64").join(lib).exists()
        } else {
            p.join("lib").join(lib).exists()
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_library_matches_platform() {
        let lib = target_library();
        if cfg!(windows) {
            assert_eq!(lib, "cudnn64_9.dll");
        } else {
            assert_eq!(lib, "libcudnn.so.9");
        }
    }

    #[test]
    fn library_search_env_matches_platform() {
        let env = library_search_env();
        if cfg!(windows) {
            assert_eq!(env, "PATH");
        } else {
            assert_eq!(env, "LD_LIBRARY_PATH");
        }
    }

    #[test]
    fn supported_on_linux_and_windows_x86_64() {
        let expected = (cfg!(target_os = "windows") || cfg!(target_os = "linux"))
            && cfg!(target_arch = "x86_64");
        assert_eq!(supported(), expected);
    }

    #[test]
    fn install_root_resolves_on_supported_platforms() {
        if supported() {
            assert!(
                install_root().is_some(),
                "install_root should resolve on {}",
                std::env::consts::OS
            );
        }
    }

    #[test]
    fn library_dir_uses_lib_on_unix_and_bin_on_windows() {
        let Some(dir) = library_dir() else {
            return;
        };
        let last = dir.file_name().and_then(|s| s.to_str()).unwrap_or_default();
        if cfg!(windows) {
            assert_eq!(last, "bin");
        } else {
            assert_eq!(last, "lib");
        }
    }
}
