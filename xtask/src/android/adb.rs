use anyhow::{Context, Result, bail};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use super::proc::{capture_timeout, run_timeout};

pub(super) fn with_device<'a>(device: Option<&'a str>, args: &[&'a str]) -> Vec<&'a str> {
    let mut all = Vec::with_capacity(args.len() + 2);
    if let Some(d) = device {
        all.push("-s");
        all.push(d);
    }
    all.extend_from_slice(args);
    all
}

pub(super) fn adb_run(device: Option<&str>, args: &[&str], timeout: Duration) -> Result<()> {
    run_timeout("adb", &with_device(device, args), timeout)
}

pub(super) fn adb_capture(
    device: Option<&str>,
    args: &[&str],
    timeout: Duration,
) -> Option<String> {
    capture_timeout("adb", &with_device(device, args), timeout)
}

pub(super) fn adb_reverse(device: Option<&str>, port: &str) -> Result<()> {
    let spec = format!("tcp:{port}");
    adb_run(device, &["reverse", &spec, &spec], Duration::from_secs(5))
}

pub(super) fn connected_devices() -> Vec<String> {
    let Some(txt) = capture_timeout("adb", &["devices"], Duration::from_secs(5)) else {
        return Vec::new();
    };
    txt.lines()
        .filter_map(|line| {
            let (id, state) = line.split_once('\t')?;
            (state.trim() == "device").then(|| id.trim().to_string())
        })
        .collect()
}

pub(super) fn attach_webview(device: Option<&str>, quiet: bool) -> Result<()> {
    let socket = adb_capture(
        device,
        &[
            "shell",
            "cat /proc/net/unix | grep -oE 'webview_devtools_remote_[0-9]+' | head -1",
        ],
        Duration::from_secs(10),
    )
    .unwrap_or_default();
    let pid = socket
        .trim()
        .strip_prefix("webview_devtools_remote_")
        .and_then(|s| s.parse::<u32>().ok())
        .context("no WebView devtools socket; is the app running?")?;
    let t = Duration::from_secs(3);
    let _ = adb_run(device, &["forward", "--remove", "tcp:9222"], t);
    let target = format!("localabstract:webview_devtools_remote_{pid}");
    adb_run(device, &["forward", "tcp:9222", &target], t)?;
    if !quiet {
        println!("forwarded tcp:9222 -> webview_devtools_remote_{pid}");
    }
    Ok(())
}

pub(super) fn wait_for_attach(device: Option<&str>, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    let mut backoff = Duration::from_millis(50);
    loop {
        match attach_webview(device, false) {
            Ok(()) => return Ok(()),
            Err(err) if Instant::now() >= deadline => return Err(err),
            Err(_) => {
                thread::sleep(backoff);
                backoff = (backoff * 2).min(Duration::from_millis(500));
            }
        }
    }
}

pub(super) fn detect_dev_host(device: Option<&str>) -> Option<String> {
    let txt = adb_capture(
        device,
        &["shell", "ip", "-4", "addr", "show", "wlan0"],
        Duration::from_secs(3),
    )?;
    let device_ip = txt
        .lines()
        .find_map(|l| l.trim().strip_prefix("inet "))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|cidr| cidr.split('/').next())?
        .to_string();
    let mut octets = device_ip.split('.');
    let a = octets.next()?;
    let b = octets.next()?;
    let c = octets.next()?;
    octets.next()?;
    let prefix = format!("{a}.{b}.{c}.");
    host_ipv4_addresses()
        .into_iter()
        .find(|ip| ip.starts_with(&prefix) && ip != &device_ip)
}

fn host_ipv4_addresses() -> Vec<String> {
    let (prog, args): (&str, &[&str]) = if cfg!(windows) {
        (
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                "Get-NetIPAddress -AddressFamily IPv4 -PrefixOrigin Dhcp,Manual -ErrorAction SilentlyContinue | Select-Object -ExpandProperty IPAddress",
            ],
        )
    } else {
        (
            "sh",
            &[
                "-c",
                "ip -4 -o addr show 2>/dev/null | awk '{print $4}' | cut -d/ -f1",
            ],
        )
    };
    Command::new(prog)
        .args(args)
        .output()
        .ok()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn detect_device_target(device: Option<&str>) -> Result<String> {
    let txt = capture_timeout("adb", &["devices"], Duration::from_secs(5))
        .context("adb devices timed out or failed")?;
    let entries: Vec<(String, String)> = txt
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(id, state)| (id.trim().to_string(), state.trim().to_string()))
        .collect();
    if entries.is_empty() {
        bail!("no adb device — connect device and enable USB debugging");
    }
    let summary = || {
        entries
            .iter()
            .map(|(id, state)| format!("{id}:{state}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    if let Some(d) = device {
        match entries.iter().find(|(id, _)| id == d) {
            Some((_, state)) if state == "device" => {}
            Some((_, state)) => bail!("adb device {d} is {state}; authorise it or reconnect it"),
            None => bail!("adb device {d} not found; connected: {}", summary()),
        }
    }
    let serials: Vec<&str> = entries
        .iter()
        .filter(|(_, state)| state == "device")
        .map(|(id, _)| id.as_str())
        .collect();
    if serials.is_empty() {
        bail!("no authorised adb device; connected: {}", summary());
    }
    if device.is_none() && serials.len() > 1 {
        bail!(
            "multiple adb devices attached: {} — pass one as `cargo xtask android dev <device>`",
            serials.join(", ")
        );
    }
    let abi = adb_capture(
        device,
        &["shell", "getprop", "ro.product.cpu.abi"],
        Duration::from_secs(5),
    )
    .context("adb shell getprop ro.product.cpu.abi timed out or failed")?;
    Ok(match abi.trim() {
        "arm64-v8a" => "aarch64".into(),
        "armeabi-v7a" | "armeabi" => "armv7".into(),
        "x86" => "i686".into(),
        "x86_64" => "x86_64".into(),
        other => bail!("unsupported device ABI: {other}"),
    })
}
