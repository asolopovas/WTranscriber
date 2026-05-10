use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

use crate::util::{exe, is_windows, root};

pub(super) struct ApkResult {
    pub path: PathBuf,
    pub signed: bool,
}

pub(super) fn find_host_bundle(ver: &str, branch: &str, dev: bool) -> Option<(PathBuf, String)> {
    let target = root()
        .join("src-tauri")
        .join("target")
        .join("release")
        .join("bundle");
    if is_windows() {
        let dir = target.join("nsis");
        if let Ok(entries) = fs::read_dir(&dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) == Some("exe")
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.ends_with("-setup.exe"))
                        .unwrap_or(false)
                {
                    let name = if dev {
                        format!("wtranscriber-setup-{branch}.exe")
                    } else {
                        format!("wtranscriber-setup-{ver}.exe")
                    };
                    return Some((p, name));
                }
            }
        }
        return None;
    }
    let dir = target.join("deb");
    if let Ok(entries) = fs::read_dir(&dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("deb") {
                let name = if dev {
                    format!("wtranscriber-{branch}_amd64.deb")
                } else {
                    format!("wtranscriber_{ver}_amd64.deb")
                };
                return Some((p, name));
            }
        }
    }
    None
}

pub(super) fn win_path_to_wsl(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    if s.len() >= 3 {
        let bytes = s.as_bytes();
        if bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/' {
            let drive = (bytes[0] as char).to_ascii_lowercase();
            return format!("/mnt/{}{}", drive, &s[2..]);
        }
    }
    s
}

pub(super) fn find_wsl_deb(ver: &str, branch: &str, dev: bool) -> Option<(PathBuf, String)> {
    let probe = std::process::Command::new("wsl")
        .args([
            "--",
            "bash",
            "-lc",
            "ls \"$HOME/.cache/wtranscriber-wsl-target/release/bundle/deb/\"*.deb 2>/dev/null | head -1",
        ])
        .output()
        .ok()?;
    let wsl_path = String::from_utf8_lossy(&probe.stdout).trim().to_string();
    if wsl_path.is_empty() {
        return None;
    }
    let to_win = std::process::Command::new("wsl")
        .args(["--", "bash", "-c", &format!("wslpath -w '{wsl_path}'")])
        .output()
        .ok()?;
    let win_path = String::from_utf8_lossy(&to_win.stdout).trim().to_string();
    if win_path.is_empty() {
        return None;
    }
    let name = if dev {
        format!("wtranscriber-{branch}_amd64.deb")
    } else {
        format!("wtranscriber_{ver}_amd64.deb")
    };
    Some((PathBuf::from(win_path), name))
}

#[allow(clippy::too_many_lines)]
pub(super) fn find_apk(dev: bool) -> Result<Option<ApkResult>> {
    let apk_dir = root()
        .join("src-tauri")
        .join("gen")
        .join("android")
        .join("app")
        .join("build")
        .join("outputs")
        .join("apk")
        .join("universal")
        .join("release");
    let signed = apk_dir.join("app-universal-release.apk");
    let unsigned = apk_dir.join("app-universal-release-unsigned.apk");
    if signed.exists() {
        let unsigned_newer = unsigned
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .zip(signed.metadata().ok().and_then(|m| m.modified().ok()))
            .map(|(u, s)| u > s)
            .unwrap_or(false);
        if !unsigned_newer {
            return Ok(Some(ApkResult {
                path: signed,
                signed: true,
            }));
        }
        let _ = fs::remove_file(&signed);
    }
    if !unsigned.exists() {
        return Ok(None);
    }

    if std::env::var_os("ANDROID_HOME").is_none()
        && !is_windows()
        && let Some(home) = std::env::var_os("HOME")
    {
        let candidate = Path::new(&home).join("Android").join("Sdk");
        if candidate.exists() {
            unsafe { std::env::set_var("ANDROID_HOME", &candidate) };
            eprintln!(
                "[and] defaulting ANDROID_HOME={} for apk signing",
                candidate.display()
            );
        }
    }
    let ks_props = root()
        .join("src-tauri")
        .join("gen")
        .join("android")
        .join("keystore.properties");
    let mut props: std::collections::HashMap<String, String> = if ks_props.exists() {
        fs::read_to_string(&ks_props)?
            .lines()
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .collect()
    } else if dev {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_default();
        let debug_ks = Path::new(&home).join(".android").join("debug.keystore");
        if !debug_ks.exists() {
            eprintln!(
                "⚠  no keystore.properties and no debug.keystore at {} — leaving APK unsigned",
                debug_ks.display()
            );
            return Ok(Some(ApkResult {
                path: unsigned,
                signed: false,
            }));
        }
        eprintln!(
            "[and] dev build: signing with debug keystore {}",
            debug_ks.display()
        );
        let mut p = std::collections::HashMap::new();
        p.insert(
            "storeFile".to_string(),
            debug_ks.to_string_lossy().to_string(),
        );
        p.insert("storePassword".to_string(), "android".to_string());
        p.insert("keyAlias".to_string(), "androiddebugkey".to_string());
        p.insert("keyPassword".to_string(), "android".to_string());
        p
    } else {
        return Ok(Some(ApkResult {
            path: unsigned,
            signed: false,
        }));
    };
    let _ = &mut props;
    let sdk = std::env::var("ANDROID_HOME").unwrap_or_else(|_| {
        let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
        format!("{local}\\Android\\Sdk")
    });
    let build_tools_dir = Path::new(&sdk).join("build-tools");
    let bt_ver = match fs::read_dir(&build_tools_dir) {
        Ok(rd) => {
            let mut versions: Vec<_> = rd
                .flatten()
                .filter_map(|e| e.file_name().into_string().ok())
                .collect();
            versions.sort();
            match versions.pop() {
                Some(v) => v,
                None => {
                    return Ok(Some(ApkResult {
                        path: unsigned,
                        signed: false,
                    }));
                }
            }
        }
        Err(_) => {
            return Ok(Some(ApkResult {
                path: unsigned,
                signed: false,
            }));
        }
    };
    let bt = build_tools_dir.join(bt_ver);
    let zipalign = bt.join(exe("zipalign"));
    let apksigner = if is_windows() {
        bt.join("apksigner.bat")
    } else {
        bt.join("apksigner")
    };
    let aligned = apk_dir.join("app-universal-release-aligned.apk");
    let out = apk_dir.join("app-universal-release.apk");
    let za = std::process::Command::new(&zipalign)
        .args([
            "-f",
            "-p",
            "4",
            unsigned.to_string_lossy().as_ref(),
            aligned.to_string_lossy().as_ref(),
        ])
        .status()?;
    if !za.success() {
        return Ok(Some(ApkResult {
            path: unsigned,
            signed: false,
        }));
    }
    let store_pass = format!(
        "pass:{}",
        props.get("storePassword").cloned().unwrap_or_default()
    );
    let key_pass = format!(
        "pass:{}",
        props.get("keyPassword").cloned().unwrap_or_default()
    );
    let aligned_str = aligned.to_string_lossy().to_string();
    let store_file = props.get("storeFile").cloned().unwrap_or_default();
    let alias = props.get("keyAlias").cloned().unwrap_or_default();
    let out_str = out.to_string_lossy().to_string();
    let sign_args: Vec<&str> = vec![
        "sign",
        "--ks",
        &store_file,
        "--ks-pass",
        &store_pass,
        "--ks-key-alias",
        &alias,
        "--key-pass",
        &key_pass,
        "--out",
        &out_str,
        &aligned_str,
    ];
    let s = std::process::Command::new(&apksigner)
        .args(&sign_args)
        .status()?;
    if !s.success() {
        return Ok(Some(ApkResult {
            path: unsigned,
            signed: false,
        }));
    }
    Ok(Some(ApkResult {
        path: out,
        signed: true,
    }))
}

pub(super) fn copy_into_channel(src: &Path, name: &str, channel_dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(channel_dir)?;
    let dst = channel_dir.join(name);
    fs::copy(src, &dst).with_context(|| format!("copy {} -> {}", src.display(), dst.display()))?;
    let size = fs::metadata(&dst)?.len() as f64 / 1024.0 / 1024.0;
    println!("  + {} ({:.1} MB)", dst.display(), size);
    Ok(dst)
}

pub(super) fn write_sha256sums(artifacts: &[PathBuf], sums_path: &Path) -> Result<()> {
    let mut lines = Vec::new();
    for p in artifacts {
        let bytes = fs::read(p)?;
        let mut h = Sha256::new();
        h.update(&bytes);
        let digest = h.finalize();
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        let name = p.file_name().context("no filename")?.to_string_lossy();
        lines.push(format!("{hex}  {name}"));
    }
    fs::write(sums_path, format!("{}\n", lines.join("\n")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn win_path_to_wsl_converts_drive_paths() {
        assert_eq!(
            win_path_to_wsl(Path::new("C:\\Users\\me\\artifact.exe")),
            "/mnt/c/Users/me/artifact.exe"
        );
    }

    #[test]
    fn win_path_to_wsl_leaves_non_drive_paths() {
        assert_eq!(win_path_to_wsl(Path::new("relative/path")), "relative/path");
    }

    #[test]
    fn write_sha256sums_uses_file_names() {
        let dir =
            std::env::temp_dir().join(format!("wtranscriber-xtask-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let artifact = dir.join("artifact.bin");
        let sums = dir.join("SHA256SUMS");
        fs::write(&artifact, b"abc").unwrap();

        write_sha256sums(std::slice::from_ref(&artifact), &sums).unwrap();

        let out = fs::read_to_string(&sums).unwrap();
        assert_eq!(
            out,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  artifact.bin\n"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
