use anyhow::{Context, Result};
use serde_json::json;
use std::fs;
use std::time::Duration;

use crate::util::root;

use super::adb::{
    adb_capture, adb_devices, adb_reverse, adb_run, attach_webview, detect_dev_host,
    detect_device_target, wait_for_attach, with_device,
};
use super::build::{preflight_node_modules, prepare};
use super::lldb;
use super::logs::{
    LogStreamer, api_probe, file_age_seconds, install_signature_mismatch, is_app_crash_signal,
    json_seconds, last_line_matching, pids_device, read_pids, tail_any,
    wait_for_log_line_with_guard,
};
use super::proc::{
    kill_pid, pid_alive, port_owner, reap_tauri_logcat_orphans, spawn_detached, spawn_with_env,
    tcp_open, wait_for_port,
};
use super::{ANDROID_PACKAGE, BootstrapMode};

pub(super) fn cmd_bootstrap(mode: BootstrapMode, device: Option<&str>) -> Result<()> {
    let tmp = root().join("tmp");
    fs::create_dir_all(&tmp)?;
    let pids_path = tmp.join("_pids.json");
    if session_already_healthy(device) {
        eprintln!(
            "BOOTSTRAP OK (already running) — use `just android-stop` first to force restart"
        );
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

    let _ = adb_run(device, &["logcat", "-c"], Duration::from_secs(5));
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

pub(super) fn cmd_status(as_json: bool, device_arg: Option<&str>) -> Result<()> {
    let tmp = root().join("tmp");
    let platform = fs::read_to_string(tmp.join("_platform")).unwrap_or_else(|_| "unknown".into());
    let pids_path = tmp.join("_pids.json");
    let pids = read_pids(&pids_path);
    let device = device_arg
        .map(str::to_string)
        .or_else(|| pids_device(&pids_path));
    let devices = adb_devices();
    let reverse = adb_capture(
        device.as_deref(),
        &["reverse", "--list"],
        Duration::from_secs(2),
    )
    .unwrap_or_default();
    let mut cdp_forward = tcp_open(9222);
    if !cdp_forward {
        let _ = attach_webview(device.as_deref(), as_json);
        cdp_forward = tcp_open(9222);
    }
    let mut api = cdp_forward
        .then(|| api_probe(Duration::from_secs(8)))
        .flatten();
    if api.is_none() {
        let _ = attach_webview(device.as_deref(), as_json);
        cdp_forward = tcp_open(9222);
        api = cdp_forward
            .then(|| api_probe(Duration::from_secs(8)))
            .flatten();
    }
    let reverse1420 = reverse.contains("tcp:1420");
    let reverse1421 = reverse.contains("tcp:1421");
    let vite_alive = tcp_open(1420);
    let api_responsive = api.is_some();
    let session_healthy = vite_alive && reverse1420 && reverse1421 && cdp_forward && api_responsive;
    let dev_log = tmp.join("android-dev.log");
    let dev_err = tmp.join("android-dev.err.log");
    let logcat_log = tmp.join("logcat.log");
    let status = json!({
        "platform": platform.trim(),
        "sessionHealthy": session_healthy,
        "pidsFile": !pids.is_empty(),
        "devWrapperPid": pids.get("dev_wrapper"),
        "devWrapperAlive": pids.get("dev_wrapper").is_some_and(|pid| pid_alive(*pid)),
        "device": device,
        "devPortOwner": pids.get("dev_port_owner"),
        "port1420Owner": port_owner(1420),
        "viteAlive": vite_alive,
        "android": {
            "adbDevices": devices,
            "reverse1420": reverse1420,
            "reverse1421": reverse1421,
            "cdpForward": cdp_forward,
            "apiResponsive": api_responsive,
            "apiProbe": api,
            "devLogAgeSeconds": file_age_seconds(&dev_log),
            "logcatAgeSeconds": file_age_seconds(&logcat_log),
            "recentViteConnection": tail_any(&[&dev_log, &dev_err], |s| (s.contains("connecting to ") || s.contains("connected to ")) && s.contains(":1420")),
            "lastHmrUpdate": last_line_matching(&dev_log, |s| s.contains("[vite] hmr update"))
                .or_else(|| last_line_matching(&dev_err, |s| s.contains("[vite] hmr update"))),
            "lastCrashSignal": last_line_matching(&logcat_log, is_app_crash_signal)
        }
    });
    if as_json {
        println!("{}", serde_json::to_string_pretty(&status)?);
        return Ok(());
    }
    let android = &status["android"];
    println!(
        "platform={} healthy={} pidsFile={} vite=:1420/{} owner={}",
        status["platform"].as_str().unwrap_or("unknown"),
        status["sessionHealthy"].as_bool().unwrap_or(false),
        status["pidsFile"].as_bool().unwrap_or(false),
        status["viteAlive"].as_bool().unwrap_or(false),
        status["port1420Owner"]
            .as_u64()
            .map_or("-".to_string(), |p| p.to_string())
    );
    println!(
        "adbDevices={} reverse1420={} reverse1421={} cdp={} api={}",
        android["adbDevices"]
            .as_array()
            .map_or(String::new(), |items| items
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(",")),
        android["reverse1420"].as_bool().unwrap_or(false),
        android["reverse1421"].as_bool().unwrap_or(false),
        android["cdpForward"].as_bool().unwrap_or(false),
        android["apiResponsive"].as_bool().unwrap_or(false)
    );
    println!(
        "devLogAge={}s logcatAge={}s viteConnectionInTail={}",
        json_seconds(&android["devLogAgeSeconds"]),
        json_seconds(&android["logcatAgeSeconds"]),
        android["recentViteConnection"].as_bool().unwrap_or(false)
    );
    if let Some(line) = android["lastHmrUpdate"].as_str() {
        println!("lastHmr={line}");
    }
    if let Some(line) = android["lastCrashSignal"].as_str() {
        println!("lastCrash={line}");
    }
    Ok(())
}

pub(super) fn cmd_stop(keep_reverse: bool, device_arg: Option<&str>) -> Result<()> {
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

pub(super) fn cmd_smoke(device: Option<&str>) -> Result<()> {
    let target = detect_device_target(device)?;
    wait_for_port(1420, Duration::from_secs(5))?;
    wait_for_attach(device, Duration::from_secs(10))?;
    let probe = api_probe(Duration::from_secs(10)).context("Tauri IPC API probe failed")?;
    println!("android smoke ok target={target} api={probe}");
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
