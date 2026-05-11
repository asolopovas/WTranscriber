use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Result, bail};

use super::config::{WindowsVmConfig, ps_single_quoted, windows_path_for_scp};
use crate::util::{SharedOut, git_short_sha, root, run_streamed};

pub(super) fn restart_vm(cfg: &WindowsVmConfig, lock: &SharedOut) -> Result<bool> {
    let command = cfg.restart_command();
    eprintln!("[win] restarting Windows VM: {}", command.join(" "));
    let rc = run_configured_command("win", &command, lock)?;
    if rc != 0 {
        eprintln!("[win] Windows VM restart failed (exit {rc})");
        return Ok(false);
    }
    wait_for_ssh(cfg, Duration::from_secs(300))
}

pub(super) fn ssh_alive(cfg: &WindowsVmConfig) -> bool {
    ssh_probe(cfg)
}

pub(super) fn build_windows_vm(
    skip: bool,
    _dev: bool,
    cfg: &WindowsVmConfig,
    lock: &SharedOut,
) -> Result<i32> {
    if skip {
        println!("[win] --skip-rebuild, leaving existing artefact alone");
        return Ok(0);
    }
    if !ssh_alive(cfg) {
        let command = cfg.start_command();
        eprintln!(
            "[win] Windows VM SSH unreachable — starting configured VM: {}",
            command.join(" ")
        );
        let rc = run_configured_command("win", &command, lock)?;
        if rc != 0 {
            eprintln!("[win] Windows VM start command failed (exit {rc})");
        }
        if !wait_for_ssh(cfg, Duration::from_secs(300))? {
            eprintln!("[win] Windows VM SSH still unreachable — attempting configured restart");
            if !restart_vm(cfg, lock)? {
                println!(
                    "[win] Windows VM SSH still unreachable after restart — skipping Windows build.\n\
                     [win]   check release.config.json windowsVm settings and run the configured VM manually."
                );
                return Ok(-1);
            }
        }
    }
    let sha = git_short_sha()?;
    let push = run_streamed("win", "git", &["push", "origin", "HEAD"], &[], lock)?;
    if push != 0 {
        eprintln!(
            "[win] git push origin HEAD failed (exit {push}) — VM cannot fetch latest commit"
        );
        return Ok(push);
    }
    let helper_local = root().join("scripts").join("wt-windows-build.bat");
    if !helper_local.exists() {
        bail!(
            "missing helper script: {} — required to drive MSVC build inside Windows VM",
            helper_local.display()
        );
    }
    let rc = build_windows_vm_once(cfg, &sha, &helper_local, lock)?;
    if rc == 0 {
        return Ok(0);
    }
    eprintln!("[win] build failed (exit {rc}); attempting configured restart + single retry");
    if !restart_vm(cfg, lock)? {
        eprintln!("[win] restart failed; not retrying");
        return Ok(rc);
    }
    eprintln!("[win] retrying build after restart");
    build_windows_vm_once(cfg, &sha, &helper_local, lock)
}

fn build_windows_vm_once(
    cfg: &WindowsVmConfig,
    sha: &str,
    helper_local: &Path,
    lock: &SharedOut,
) -> Result<i32> {
    let mkdir_script = format!(
        "New-Item -ItemType Directory -Force -Path {} | Out-Null",
        cfg.remote_work_dir_ps()
    );
    let mkdir = run_streamed("win", "ssh", &[&cfg.ssh_host, &mkdir_script], &[], lock)?;
    if mkdir != 0 {
        eprintln!("[win] failed to create remote work dir on VM (exit {mkdir})");
        return Ok(mkdir);
    }
    let helper_remote = cfg.helper_remote_scp_path();
    let scp = run_streamed(
        "win",
        "scp",
        &[helper_local.to_string_lossy().as_ref(), &helper_remote],
        &[],
        lock,
    )?;
    if scp != 0 {
        eprintln!("[win] scp helper to VM failed (exit {scp})");
        return Ok(scp);
    }
    let build_cmd = format!(
        "cmd /c \"\"{}\" {} \"{}\"\"",
        cfg.helper_remote_cmd_path(),
        sha,
        cfg.remote_repo_dir
    );
    run_streamed("win", "ssh", &[&cfg.ssh_host, &build_cmd], &[], lock)
}

pub(super) fn fetch_windows_vm_exe(
    cfg: &WindowsVmConfig,
    ver: &str,
    branch: &str,
    dev: bool,
    out_channel_dir: &Path,
) -> Result<Option<PathBuf>> {
    let pattern = format!(
        "{}\\src-tauri\\target\\release\\bundle\\nsis\\*-setup.exe",
        cfg.remote_repo_dir
    );
    let probe_script = format!(
        "Get-ChildItem -Path {} -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty FullName",
        ps_single_quoted(&pattern)
    );
    let probe = std::process::Command::new("ssh")
        .args([&cfg.ssh_host, &probe_script])
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
    let scp_src = format!("{}:{}", cfg.ssh_host, windows_path_for_scp(&remote_path));
    let st = std::process::Command::new("scp")
        .args([&scp_src, dst.to_string_lossy().as_ref()])
        .status()?;
    if !st.success() {
        bail!("scp from Windows VM failed (exit {:?})", st.code());
    }
    let size = fs::metadata(&dst)?.len() as f64 / 1024.0 / 1024.0;
    println!("  + {} ({:.1} MB) (windows-vm)", dst.display(), size);
    Ok(Some(dst))
}

fn wait_for_ssh(cfg: &WindowsVmConfig, timeout: Duration) -> Result<bool> {
    eprintln!(
        "[win] waiting up to {}s for Windows VM SSH ({})",
        timeout.as_secs(),
        cfg.ssh_host
    );
    let deadline = std::time::Instant::now() + timeout;
    let mut attempts = 0u32;
    while std::time::Instant::now() < deadline {
        attempts += 1;
        if ssh_probe(cfg) {
            eprintln!("[win] Windows VM SSH responded after {attempts} attempt(s)");
            return Ok(true);
        }
        std::thread::sleep(Duration::from_secs(5));
    }
    eprintln!(
        "[win] Windows VM SSH did not respond within {}s",
        timeout.as_secs()
    );
    Ok(false)
}

fn ssh_probe(cfg: &WindowsVmConfig) -> bool {
    std::process::Command::new("ssh")
        .args([
            "-o",
            "ConnectTimeout=5",
            "-o",
            "BatchMode=yes",
            &cfg.ssh_host,
            &cfg.ssh_ready_command,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn run_configured_command(tag: &str, command: &[String], lock: &SharedOut) -> Result<i32> {
    if command.is_empty() {
        bail!("configured command for {tag} is empty");
    }
    let args = command[1..].iter().map(String::as_str).collect::<Vec<_>>();
    run_streamed(tag, &command[0], &args, &[], lock)
}
