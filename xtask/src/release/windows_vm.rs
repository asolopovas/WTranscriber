use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use crate::util::{SharedOut, git_short_sha, root, run_streamed};

pub(super) fn ssh_alive() -> bool {
    std::process::Command::new("ssh")
        .args([
            "-o",
            "ConnectTimeout=5",
            "-o",
            "BatchMode=yes",
            "windows-vm",
            "true",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub(super) fn build_windows_vm(skip: bool, _dev: bool, lock: &SharedOut) -> Result<i32> {
    if skip {
        println!("[win] --skip-rebuild, leaving existing artefact alone");
        return Ok(0);
    }
    if !ssh_alive() {
        println!(
            "[win] windows-vm SSH unreachable on localhost:2222 — skipping Windows build.\n\
             [win]   bring up: cd ~/os/windows-vm && make up   (then run shared/enable-ssh.ps1 inside VM once)"
        );
        return Ok(-1);
    }
    let sha = git_short_sha()?;
    let push = run_streamed("win", "git", &["push", "origin", "HEAD"], &[], lock)?;
    if push != 0 {
        eprintln!("[win] git push origin HEAD failed (exit {push}) — VM cannot fetch latest commit");
        return Ok(push);
    }
    let helper_local = root().join("scripts").join("wt-windows-build.bat");
    if !helper_local.exists() {
        bail!(
            "missing helper script: {} — required to drive MSVC build inside windows-vm",
            helper_local.display()
        );
    }
    let mkdir = run_streamed(
        "win",
        "ssh",
        &["windows-vm", "mkdir", "-p", "/c/wt-build"],
        &[],
        lock,
    )?;
    if mkdir != 0 {
        eprintln!("[win] failed to create /c/wt-build on VM (exit {mkdir})");
        return Ok(mkdir);
    }
    let scp = run_streamed(
        "win",
        "scp",
        &[
            helper_local.to_string_lossy().as_ref(),
            "windows-vm:C:/wt-build/wt-windows-build.bat",
        ],
        &[],
        lock,
    )?;
    if scp != 0 {
        eprintln!("[win] scp helper to VM failed (exit {scp})");
        return Ok(scp);
    }
    run_streamed(
        "win",
        "ssh",
        &[
            "windows-vm",
            "cmd",
            "//c",
            &format!("C:/wt-build/wt-windows-build.bat {sha}"),
        ],
        &[],
        lock,
    )
}

pub(super) fn fetch_windows_vm_exe(
    ver: &str,
    branch: &str,
    dev: bool,
    out_channel_dir: &Path,
) -> Result<Option<PathBuf>> {
    let probe = std::process::Command::new("ssh")
        .args([
            "windows-vm",
            "bash -lc 'ls /c/WTranscriber/src-tauri/target/release/bundle/nsis/*-setup.exe 2>/dev/null | head -1'",
        ])
        .output()?;
    let remote_path = String::from_utf8_lossy(&probe.stdout)
        .lines()
        .map(|l| l.trim())
        .find(|l| l.contains("-setup.exe"))
        .unwrap_or("")
        .to_string();
    if remote_path.is_empty() {
        return Ok(None);
    }
    fs::create_dir_all(out_channel_dir)?;
    let dst_name = if dev {
        format!("wtranscriber-setup-{branch}.exe")
    } else {
        format!("wtranscriber-setup-{ver}.exe")
    };
    let dst = out_channel_dir.join(&dst_name);
    let scp_src = match remote_path.strip_prefix("/c/") {
        Some(stripped) => format!("windows-vm:C:/{stripped}"),
        None => format!("windows-vm:{remote_path}"),
    };
    let st = std::process::Command::new("scp")
        .args([&scp_src, dst.to_string_lossy().as_ref()])
        .status()?;
    if !st.success() {
        bail!("scp from windows-vm failed (exit {:?})", st.code());
    }
    let size = fs::metadata(&dst)?.len() as f64 / 1024.0 / 1024.0;
    println!("  + {} ({:.1} MB) (windows-vm)", dst.display(), size);
    Ok(Some(dst))
}
