use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::util::root;

use super::ANDROID_PACKAGE;
use super::adb::{adb_capture, adb_run};
use super::proc::spawn_detached;

pub(super) struct LldbInfo {
    pub app_pid: u32,
    pub server_pid: u32,
    pub host_port: u16,
}

pub(super) fn attach(device: Option<&str>) -> Result<LldbInfo> {
    let abi = adb_capture(
        device,
        &["shell", "getprop", "ro.product.cpu.abi"],
        Duration::from_secs(5),
    )
    .context("adb getprop ro.product.cpu.abi failed")?;
    let abi = abi.trim();
    let arch = match abi {
        "arm64-v8a" => "aarch64",
        "armeabi-v7a" | "armeabi" => "arm",
        "x86" => "i386",
        "x86_64" => "x86_64",
        other => bail!("unsupported device ABI for lldb: {other}"),
    };

    let server = locate_lldb_server(arch)?;

    let app_pid_raw = adb_capture(
        device,
        &["shell", "pidof", ANDROID_PACKAGE],
        Duration::from_secs(5),
    )
    .context("adb pidof failed")?;
    let app_pid: u32 = app_pid_raw
        .split_whitespace()
        .next()
        .and_then(|s| s.parse().ok())
        .with_context(|| format!("{ANDROID_PACKAGE} is not running (pidof empty)"))?;

    let server_str = server.to_string_lossy().to_string();
    adb_run(
        device,
        &["push", &server_str, "/data/local/tmp/lldb-server"],
        Duration::from_secs(60),
    )
    .context("adb push lldb-server failed")?;

    let cp_cmd = format!(
        "run-as {pkg} cp /data/local/tmp/lldb-server ./lldb-server && run-as {pkg} chmod 700 lldb-server",
        pkg = ANDROID_PACKAGE
    );
    adb_run(device, &["shell", &cp_cmd], Duration::from_secs(15))
        .context("adb shell run-as cp/chmod lldb-server failed")?;

    let tmp = root().join("tmp");
    fs::create_dir_all(&tmp)?;
    let unix_path = format!("unix-abstract:///{ANDROID_PACKAGE}-lldb");
    let shell_cmd = format!(
        "run-as {pkg} ./lldb-server platform --listen {unix} --server",
        pkg = ANDROID_PACKAGE,
        unix = unix_path
    );
    let mut adb_args: Vec<&str> = Vec::new();
    if let Some(d) = device {
        adb_args.push("-s");
        adb_args.push(d);
    }
    adb_args.push("shell");
    adb_args.push(&shell_cmd);
    let server_pid = spawn_detached(
        "adb",
        &adb_args,
        &[],
        &tmp.join("lldb-server.log"),
        &tmp.join("lldb-server.err.log"),
    )?;

    let forward_target = format!("localabstract:{ANDROID_PACKAGE}-lldb");
    let _ = adb_run(
        device,
        &["forward", "--remove", "tcp:5039"],
        Duration::from_secs(3),
    );
    adb_run(
        device,
        &["forward", "tcp:5039", &forward_target],
        Duration::from_secs(5),
    )
    .context("adb forward tcp:5039 failed")?;

    Ok(LldbInfo {
        app_pid,
        server_pid,
        host_port: 5039,
    })
}

pub(super) fn cleanup(device: Option<&str>, _app_pid_hint: Option<u32>) {
    let _ = adb_run(
        device,
        &["forward", "--remove", "tcp:5039"],
        Duration::from_secs(3),
    );
    let kill_cmd = format!("run-as {ANDROID_PACKAGE} killall lldb-server");
    let _ = adb_run(device, &["shell", &kill_cmd], Duration::from_secs(5));
}

pub(super) fn write_vscode_launch(app_pid: u32, port: u16) -> Result<()> {
    let dir = root().join(".vscode");
    fs::create_dir_all(&dir)?;
    let path = dir.join("launch.json");
    let name = "Android: Attach Tauri (lldb)";
    let new_cfg = json!({
        "name": name,
        "type": "lldb",
        "request": "attach",
        "pid": app_pid,
        "initCommands": [
            "platform select remote-android",
            format!("platform connect connect://localhost:{port}")
        ]
    });

    let mut root_val: Value = fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| json!({ "version": "0.2.0", "configurations": [] }));

    if !root_val.is_object() {
        root_val = json!({ "version": "0.2.0", "configurations": [] });
    }
    let obj = root_val.as_object_mut().unwrap();
    obj.entry("version")
        .or_insert_with(|| Value::String("0.2.0".into()));
    let configs = obj
        .entry("configurations")
        .or_insert_with(|| Value::Array(Vec::new()));
    if !configs.is_array() {
        *configs = Value::Array(Vec::new());
    }
    let arr = configs.as_array_mut().unwrap();
    let mut replaced = false;
    for c in arr.iter_mut() {
        if c.get("name").and_then(Value::as_str) == Some(name) {
            *c = new_cfg.clone();
            replaced = true;
            break;
        }
    }
    if !replaced {
        arr.push(new_cfg);
    }

    fs::write(&path, serde_json::to_string_pretty(&root_val)?)?;
    Ok(())
}

fn locate_lldb_server(arch: &str) -> Result<PathBuf> {
    let ndk = std::env::var("NDK_HOME")
        .or_else(|_| std::env::var("ANDROID_NDK_HOME"))
        .or_else(|_| std::env::var("ANDROID_NDK_ROOT"))
        .context("NDK_HOME / ANDROID_NDK_HOME not set")?;
    let host = if cfg!(target_os = "windows") {
        "windows-x86_64"
    } else if cfg!(target_os = "macos") {
        "darwin-x86_64"
    } else {
        "linux-x86_64"
    };
    let clang_root = Path::new(&ndk)
        .join("toolchains")
        .join("llvm")
        .join("prebuilt")
        .join(host)
        .join("lib")
        .join("clang");
    if !clang_root.exists() {
        bail!("NDK clang dir not found: {}", clang_root.display());
    }
    let mut best: Option<(Vec<u32>, PathBuf)> = None;
    for entry in fs::read_dir(&clang_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let parts: Vec<u32> = name.split('.').filter_map(|p| p.parse().ok()).collect();
        if parts.is_empty() {
            continue;
        }
        let candidate = entry
            .path()
            .join("lib")
            .join("linux")
            .join(arch)
            .join("lldb-server");
        if !candidate.exists() {
            continue;
        }
        if best.as_ref().is_none_or(|(v, _)| &parts > v) {
            best = Some((parts, candidate));
        }
    }
    best.map(|(_, p)| p).with_context(|| {
        format!(
            "lldb-server not found under {} for arch {arch}",
            clang_root.display()
        )
    })
}
