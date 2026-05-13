use anyhow::{Context, Result, bail};
use std::fs::{self, File};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::util::root;

pub(super) fn wait_output(child: Child, timeout: Duration) -> Option<Output> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(output)) => Some(output),
        _ => None,
    }
}

pub(super) fn run_timeout(prog: &str, args: &[&str], timeout: Duration) -> Result<()> {
    let child = Command::new(prog)
        .args(args)
        .current_dir(root())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawn {prog}"))?;
    match wait_output(child, timeout) {
        Some(out) if out.status.success() => Ok(()),
        Some(out) => bail!(
            "{} {:?} failed: {}",
            prog,
            args,
            String::from_utf8_lossy(&out.stderr).trim()
        ),
        None => bail!("{} {:?} timed out after {}s", prog, args, timeout.as_secs()),
    }
}

pub(super) fn capture_timeout(prog: &str, args: &[&str], timeout: Duration) -> Option<String> {
    let child = Command::new(prog)
        .args(args)
        .current_dir(root())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let out = wait_output(child, timeout)?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub(super) fn spawn_persistent(
    prog: &str,
    args: &[&str],
    env: &[(String, String)],
    stdout_path: &Path,
    stderr_path: &Path,
) -> Result<u32> {
    if cfg!(windows) {
        if let Some(parent) = stdout_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let cmd_quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        let batch_path = stdout_path.with_extension("cmd");
        let mut batch = String::from("@echo off\r\n");
        for (k, v) in env {
            batch.push_str(&format!("set \"{k}={v}\"\r\n"));
        }
        batch.push_str(&format!(
            "cd /d {}\r\n",
            cmd_quote(&root().to_string_lossy())
        ));
        batch.push_str(&cmd_quote(prog));
        for arg in args {
            batch.push(' ');
            batch.push_str(&cmd_quote(arg));
        }
        batch.push_str(&format!(
            " > {} 2> {}\r\n",
            cmd_quote(&stdout_path.to_string_lossy()),
            cmd_quote(&stderr_path.to_string_lossy())
        ));
        fs::write(&batch_path, batch)?;
        let quote = |s: &str| format!("'{}'", s.replace('\'', "''"));
        let command_line = format!("cmd /C {}", cmd_quote(&batch_path.to_string_lossy()));
        let script = format!(
            "$r=Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{{CommandLine={};CurrentDirectory={}}}; if($r.ReturnValue -ne 0){{throw \"Win32_Process.Create failed $($r.ReturnValue)\"}}; $r.ProcessId",
            quote(&command_line),
            quote(&root().to_string_lossy())
        );
        let out = Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .current_dir(root())
            .output()
            .context("spawn persistent process failed")?;
        if !out.status.success() {
            bail!(
                "spawn persistent process failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        return String::from_utf8_lossy(&out.stdout)
            .lines()
            .find_map(|line| line.trim().parse::<u32>().ok())
            .context("spawn persistent process did not return a pid");
    }
    spawn_detached(prog, args, env, stdout_path, stderr_path)
}

pub(super) fn spawn_detached(
    prog: &str,
    args: &[&str],
    env: &[(String, String)],
    stdout_path: &Path,
    stderr_path: &Path,
) -> Result<u32> {
    if let Some(parent) = stdout_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let build = |flags: u32| -> Result<Child> {
        let mut cmd = Command::new(prog);
        cmd.args(args)
            .current_dir(root())
            .stdin(Stdio::null())
            .stdout(Stdio::from(File::create(stdout_path)?))
            .stderr(Stdio::from(File::create(stderr_path)?));
        for (k, v) in env {
            cmd.env(k, v);
        }
        #[cfg(windows)]
        cmd.creation_flags(flags);
        #[cfg(not(windows))]
        let _ = flags;
        cmd.spawn().with_context(|| format!("spawn {prog}"))
    };
    let primary = 0x0800_0000 | 0x0100_0000 | 0x0000_0008 | 0x0000_0200;
    let fallback = 0x0800_0000;
    let child = match build(primary) {
        Ok(c) => c,
        Err(_) => build(fallback)?,
    };
    Ok(child.id())
}

pub(super) fn spawn_with_env(prog: &str, args: &[&str], env: &[(String, String)]) -> Result<()> {
    let mut cmd = Command::new(prog);
    cmd.args(args).current_dir(root());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let status = cmd.status().with_context(|| format!("spawn {prog}"))?;
    if !status.success() {
        bail!("{} {:?} exited with {:?}", prog, args, status.code());
    }
    Ok(())
}

pub(crate) fn port_owner(port: u16) -> Option<u32> {
    if !cfg!(windows) {
        return None;
    }
    let pattern = format!(":{port}");
    let out = capture_timeout("netstat", &["-ano"], Duration::from_secs(2))?;
    out.lines()
        .find(|line| line.contains(&pattern) && line.contains("LISTENING"))
        .and_then(|line| line.split_whitespace().last())
        .and_then(|pid| pid.parse::<u32>().ok())
}

pub(super) fn tcp_open(port: u16) -> bool {
    TcpStream::connect_timeout(
        &std::net::SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, port)),
        Duration::from_millis(100),
    )
    .is_ok()
}

pub(super) fn pid_alive(pid: u32) -> bool {
    let pid_text = pid.to_string();
    if cfg!(windows) {
        capture_timeout(
            "tasklist",
            &["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"],
            Duration::from_secs(2),
        )
        .is_some_and(|out| out.contains(&pid_text))
    } else {
        Command::new("kill")
            .args(["-0", &pid_text])
            .status()
            .is_ok_and(|s| s.success())
    }
}

pub(crate) fn kill_pid(pid: u32) {
    let pid_text = pid.to_string();
    if cfg!(windows) {
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid_text])
            .status();
    } else {
        let _ = Command::new("kill").args(["-TERM", &pid_text]).status();
    }
}

pub(super) fn reap_tauri_logcat_orphans() {
    if !cfg!(windows) {
        return;
    }
    let Some(out) = capture_timeout(
        "powershell",
        &[
            "-NoProfile",
            "-Command",
            "Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'adb.exe' -and $_.CommandLine -match 'logcat .* -s wtranscriber' } | ForEach-Object { $_.ProcessId }",
        ],
        Duration::from_secs(3),
    ) else {
        return;
    };
    for pid in out.lines().filter_map(|l| l.trim().parse::<u32>().ok()) {
        kill_pid(pid);
        eprintln!("reaped orphan tauri logcat pid={pid}");
    }
}
