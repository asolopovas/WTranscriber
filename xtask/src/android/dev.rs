use anyhow::Result;
use serde_json::json;
use std::fs;
use std::time::Duration;

use crate::util::root;

use super::adb::{
    adb_capture, adb_reverse, adb_run, attach_webview, detect_dev_host, detect_device_target,
    wait_for_attach, with_device,
};
use super::build::{preflight_node_modules, prepare};
use super::lldb;
use super::logs::{
    LogStreamer, api_probe, install_signature_mismatch, pids_device, read_pids,
    wait_for_log_line_with_guard,
};
use super::proc::{
    kill_pid, pid_alive, port_owner, reap_tauri_logcat_orphans, spawn_detached, spawn_with_env,
    tcp_open,
};
use super::{ANDROID_PACKAGE, BootstrapMode};

pub(super) fn cmd_bootstrap(mode: BootstrapMode, device: Option<&str>) -> Result<()> {
    let tmp = root().join("tmp");
    fs::create_dir_all(&tmp)?;
    let pids_path = tmp.join("_pids.json");
    if session_already_healthy(device) {
        eprintln!("BOOTSTRAP OK (already running) — use `just dev stop` first to force restart");
        return Ok(());
    }
    if pids_path.exists() {
        eprintln!("[stage 0/6] previous session unhealthy — stopping");
        cmd_stop(false, device)?;
    } else if tcp_open(1420) {
        eprintln!("[stage 0/6] zombie vite on :1420 — stopping");
        cmd_stop(false, device)?;
    }
    reap_tauri_logcat_orphans();
    eprintln!("[stage 1/6] preflight (node_modules, device)");
    preflight_node_modules()?;
    detect_device_target(device)?;
    fs::write(tmp.join("_platform"), "android")?;

    let _ = adb_run(
        device,
        &["logcat", "-b", "main,events", "-c"],
        Duration::from_secs(5),
    );
    let logcat_args: Vec<String> = with_device(
        device,
        &[
            "logcat",
            "-b",
            "main,events",
            "*:W",
            "RustStdoutStderr:V",
            "Tauri:V",
            "chromium:V",
            "am_crash:V",
            "am_proc_died:V",
            "am_proc_start:V",
            "am_kill:V",
        ],
    )
    .into_iter()
    .map(String::from)
    .collect();
    let logcat_arg_refs: Vec<&str> = logcat_args.iter().map(String::as_str).collect();
    eprintln!("[stage 2/6] starting logcat capture");
    let logcat_pid = spawn_detached(
        "adb",
        &logcat_arg_refs,
        &[],
        &tmp.join("logcat.log"),
        &tmp.join("logcat.err.log"),
    )?;
    let vital_pid = spawn_detached(
        "bun",
        &["scripts/dev-vital.ts"],
        &[],
        &tmp.join("dev-vital.out.log"),
        &tmp.join("dev-vital.err.log"),
    )?;

    let mut env = Vec::<(String, String)>::new();
    let mut args = vec![
        "xtask".to_string(),
        "android".to_string(),
        "dev".to_string(),
    ];
    match mode {
        BootstrapMode::Usb => {
            env.push(("TAURI_DEV_HOST".into(), "127.0.0.1".into()));
            adb_reverse(device, "1420")?;
            adb_reverse(device, "1421")?;
        }
        BootstrapMode::Host => args.push("--host".into()),
    }
    if let Some(d) = device {
        args.push(d.to_string());
    }
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    eprintln!("[stage 3/6] spawning tauri android dev (logs: tmp/android-dev.{{log,err.log}})");
    let dev_pid = spawn_detached(
        "cargo",
        &arg_refs,
        &env,
        &tmp.join("android-dev.log"),
        &tmp.join("android-dev.err.log"),
    )?;

    let dev_log = tmp.join("android-dev.log");
    let dev_err = tmp.join("android-dev.err.log");
    let logcat_log = tmp.join("logcat.log");
    let streamer = LogStreamer::start(vec![dev_log.clone(), dev_err.clone()]);
    let alive = || pid_alive(dev_pid);
    let healthy = || {
        if !pid_alive(dev_pid) {
            return false;
        }
        if install_signature_mismatch(&[&dev_log, &dev_err]) {
            return false;
        }
        true
    };
    let bring_up = || -> Result<lldb::LldbInfo> {
        eprintln!(
            "[stage 4/7] waiting for vite :1420 (event: \"ready in\"/\"Local:\" in tmp/android-dev.log, ≤90s)"
        );
        wait_for_log_line_with_guard(
            &[&dev_log, &dev_err],
            "vite ready on :1420",
            |s| s.contains("Local:") && s.contains(":1420"),
            Duration::from_secs(90),
            healthy,
        )?;
        eprintln!(
            "[stage 5a/7] waiting for cargo+gradle build → APK install/launch (event: \"Info Opening\"/\"Finished\"/am_proc_start in logs, ≤1800s)"
        );
        wait_for_log_line_with_guard(
            &[&dev_log, &dev_err, &logcat_log],
            "APK install/launch",
            |s| {
                s.contains("Info Opening ")
                    || s.contains("Info Installing")
                    || s.contains("Performing Streamed Install")
                    || (s.contains("Starting: Intent") && s.contains("wtranscriber"))
                    || (s.contains("am_proc_start") && s.contains("wtranscriber"))
            },
            Duration::from_secs(1800),
            healthy,
        )?;
        eprintln!(
            "[stage 5b/7] waiting for WebView → :1420 connection (event: connecting/connected to :1420 in logcat, ≤90s)"
        );
        wait_for_log_line_with_guard(
            &[&dev_log, &dev_err, &logcat_log],
            "WebView connecting to :1420",
            |s| {
                (s.contains("connecting to ") || s.contains("connected to ")) && s.contains(":1420")
            },
            Duration::from_secs(90),
            healthy,
        )?;
        eprintln!("[stage 6/7] attaching CDP and probing tauri IPC");
        wait_for_attach(device, Duration::from_secs(10))?;
        match api_probe(Duration::from_secs(20)) {
            Some(_) => {}
            None => eprintln!(
                "warning: Tauri IPC probe did not return within 20s; session is up (WebView connected to :1420), continuing"
            ),
        }
        eprintln!("[stage 7/7] attaching lldb (blocking)");
        let info = lldb::attach(device)?;
        lldb::write_vscode_launch(info.app_pid, info.host_port)?;
        eprintln!(
            "  ✓ lldb attached app_pid={} server_pid={} port={}",
            info.app_pid, info.server_pid, info.host_port
        );
        Ok(info)
    };
    let bring_up_result = bring_up();
    streamer.stop();
    let _ = alive;
    let info = match bring_up_result {
        Ok(info) => info,
        Err(err) => {
            if install_signature_mismatch(&[&dev_log, &dev_err]) {
                eprintln!(
                    "detected APK signature mismatch — uninstalling {ANDROID_PACKAGE} and retrying once"
                );
                kill_pid(dev_pid);
                kill_pid(logcat_pid);
                kill_pid(vital_pid);
                reap_tauri_logcat_orphans();
                let _ = adb_run(
                    device,
                    &["uninstall", ANDROID_PACKAGE],
                    Duration::from_secs(30),
                );
                let _ = fs::remove_file(&pids_path);
                return cmd_bootstrap(mode, device);
            }
            eprintln!("bootstrap failed: {err:#}");
            eprintln!("--- last 10 lines of android-dev.err.log ---");
            if let Ok(raw) = fs::read_to_string(&dev_err) {
                for line in raw.lines().rev().take(10).collect::<Vec<_>>().iter().rev() {
                    eprintln!("  {line}");
                }
            }
            kill_pid(dev_pid);
            kill_pid(logcat_pid);
            kill_pid(vital_pid);
            lldb::cleanup(device, None);
            reap_tauri_logcat_orphans();
            let _ = adb_run(
                device,
                &["forward", "--remove", "tcp:9222"],
                Duration::from_secs(3),
            );
            let _ = adb_run(
                device,
                &["reverse", "--remove", "tcp:1420"],
                Duration::from_secs(3),
            );
            let _ = adb_run(
                device,
                &["reverse", "--remove", "tcp:1421"],
                Duration::from_secs(3),
            );
            let _ = fs::remove_file(tmp.join("_platform"));
            return Err(err);
        }
    };
    let pids = json!({
        "device": device,
        "dev_wrapper": dev_pid,
        "dev_port_owner": port_owner(1420),
        "logcat": logcat_pid,
        "vital": vital_pid,
        "lldb_server": info.server_pid,
        "lldb_port": info.host_port,
        "app_pid": info.app_pid
    });
    fs::write(tmp.join("_pids.json"), serde_json::to_string(&pids)?)?;
    println!(
        "BOOTSTRAP OK platform=android mode={} pids={}",
        match mode {
            BootstrapMode::Usb => "usb",
            BootstrapMode::Host => "host",
        },
        pids
    );
    Ok(())
}

fn session_already_healthy(device_arg: Option<&str>) -> bool {
    if !tcp_open(1420) {
        return false;
    }
    let reverse =
        adb_capture(device_arg, &["reverse", "--list"], Duration::from_secs(2)).unwrap_or_default();
    if !reverse.contains("tcp:1420") {
        return false;
    }
    if !tcp_open(9222) {
        let _ = attach_webview(device_arg, true);
        if !tcp_open(9222) {
            return false;
        }
    }
    api_probe(Duration::from_secs(5)).is_some()
}

pub(crate) fn cmd_stop(keep_reverse: bool, device_arg: Option<&str>) -> Result<()> {
    let tmp = root().join("tmp");
    let pids_path = tmp.join("_pids.json");
    let pids = read_pids(&pids_path);
    let device = device_arg
        .map(str::to_string)
        .or_else(|| pids_device(&pids_path));
    for key in [
        "vital",
        "lldb_server",
        "logcat",
        "dev_wrapper",
        "dev_port_owner",
    ] {
        if let Some(pid) = pids.get(key) {
            kill_pid(*pid);
            println!("stopped {key} pid={pid}");
        }
    }
    lldb::cleanup(device.as_deref(), pids.get("app_pid").copied());
    reap_tauri_logcat_orphans();
    if !keep_reverse {
        let d = device.as_deref();
        let t = Duration::from_secs(3);
        let _ = adb_run(d, &["forward", "--remove", "tcp:9222"], t);
        let _ = adb_run(d, &["reverse", "--remove", "tcp:1420"], t);
        let _ = adb_run(d, &["reverse", "--remove", "tcp:1421"], t);
    }
    let _ = fs::remove_file(tmp.join("_pids.json"));
    let _ = fs::remove_file(tmp.join("_platform"));
    println!("dev session stopped");
    Ok(())
}

pub(super) fn cmd_dev(open: bool, host: bool, watch: bool, device: Option<&str>) -> Result<()> {
    let target = detect_device_target(device)?;
    println!("android dev: detected ABI → target={target}");
    let mut env = prepare(&target, false)?;
    if std::env::var_os("TAURI_DEV_HOST").is_none()
        && let Some(ip) = detect_dev_host(device)
    {
        println!("android dev: auto-detected TAURI_DEV_HOST={ip}");
        env.push(("TAURI_DEV_HOST".into(), ip));
    }
    let dev_host_arg = std::env::var("TAURI_DEV_HOST").ok();
    let mut tauri_args: Vec<&str> = vec!["run", "tauri", "android", "dev"];
    if open {
        tauri_args.push("--open");
    }
    if host {
        tauri_args.push("--host");
    } else if let Some(ref value) = dev_host_arg {
        tauri_args.push("--host");
        tauri_args.push(value);
    }
    if !watch {
        tauri_args.push("--no-watch");
    }
    let tauri_device_name = device.map(|d| {
        if let Some(rest) = d.strip_prefix("emulator-") {
            let _ = rest;
            super::proc::capture_timeout(
                "adb",
                &["-s", d, "emu", "avd", "name"],
                Duration::from_secs(3),
            )
            .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| d.to_string())
        } else {
            d.to_string()
        }
    });
    if let Some(ref name) = tauri_device_name {
        tauri_args.push(name);
    }
    tauri_args.extend_from_slice(&["--", "--no-default-features", "--features", "android"]);
    if let Some(d) = device {
        env.push(("ANDROID_SERIAL".into(), d.to_string()));
    }
    spawn_with_env("bun", &tauri_args, &env)
}
