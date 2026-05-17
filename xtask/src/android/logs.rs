use anyhow::{Result, bail};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use super::proc::capture_timeout;

pub(super) fn last_line_matching(path: &Path, f: impl Fn(&str) -> bool) -> Option<String> {
    fs::read_to_string(path)
        .ok()?
        .lines()
        .rev()
        .take(500)
        .find(|line| f(line))
        .map(str::to_string)
}

pub(super) fn tail_any(paths: &[&Path], f: impl Fn(&str) -> bool) -> bool {
    paths.iter().any(|p| last_line_matching(p, &f).is_some())
}

pub(super) fn wait_for_log_line_with_guard(
    paths: &[&Path],
    label: &str,
    f: impl Fn(&str) -> bool,
    timeout: Option<Duration>,
    guard: impl Fn() -> bool,
) -> Result<()> {
    let start = Instant::now();
    let deadline = timeout.map(|timeout| start + timeout);
    if tail_any(paths, &f) {
        eprintln!("  ✓ {label} (0s)");
        return Ok(());
    }
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel::<String>();
    let owned: Vec<PathBuf> = paths.iter().map(|p| (*p).to_path_buf()).collect();
    let stop_tail = Arc::clone(&stop);
    let tail = thread::spawn(move || tail_lines(owned, tx, stop_tail));
    let mut last_line: Option<String> = None;
    let mut last_progress = Instant::now();
    let result = loop {
        let now = Instant::now();
        if let Some(deadline) = deadline
            && now >= deadline
        {
            break Err(());
        }
        let slice = deadline
            .map(|deadline| (deadline - now).min(Duration::from_secs(1)))
            .unwrap_or_else(|| Duration::from_secs(1));
        match rx.recv_timeout(slice) {
            Ok(line) => {
                if f(&line) {
                    eprintln!("  ✓ {label} ({}s)", start.elapsed().as_secs());
                    break Ok(());
                }
                if last_progress.elapsed() >= Duration::from_secs(5)
                    && let Some(display) = console_log_line(&line)
                {
                    if last_line.as_deref() != Some(display.as_str()) {
                        eprintln!("  [{:>3}s] {display}", start.elapsed().as_secs());
                        last_line = Some(display);
                    }
                    last_progress = Instant::now();
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if !guard() {
                    break Err(());
                }
            }
            Err(RecvTimeoutError::Disconnected) => break Err(()),
        }
    };
    stop.store(true, Ordering::Relaxed);
    let _ = tail.join();
    match result {
        Ok(()) => Ok(()),
        Err(()) if !guard() => {
            bail!("{label} aborted — child process exited; see {paths:?} for details")
        }
        Err(()) => match timeout {
            Some(timeout) => bail!(
                "{label} not seen in {paths:?} within {}s — check adb reverse / TAURI_DEV_HOST / device app launch",
                timeout.as_secs()
            ),
            None => bail!(
                "{label} not seen in {paths:?} — check adb reverse / TAURI_DEV_HOST / device app launch"
            ),
        },
    }
}

fn tail_lines(paths: Vec<PathBuf>, tx: mpsc::Sender<String>, stop: Arc<AtomicBool>) {
    let mut offsets: Vec<u64> = paths
        .iter()
        .map(|p| fs::metadata(p).map(|m| m.len()).unwrap_or(0))
        .collect();
    while !stop.load(Ordering::Relaxed) {
        let mut produced = false;
        for (i, p) in paths.iter().enumerate() {
            let Ok(meta) = fs::metadata(p) else { continue };
            let len = meta.len();
            if len <= offsets[i] {
                if len < offsets[i] {
                    offsets[i] = 0;
                }
                continue;
            }
            let Ok(raw) = fs::read(p) else { continue };
            let from = offsets[i] as usize;
            if from >= raw.len() {
                continue;
            }
            if let Ok(text) = std::str::from_utf8(&raw[from..]) {
                for line in text.lines() {
                    let cleaned = strip_ansi(line);
                    let trimmed = cleaned.trim_end();
                    if trimmed.is_empty() {
                        continue;
                    }
                    produced = true;
                    if tx.send(trimmed.to_string()).is_err() {
                        return;
                    }
                }
                offsets[i] = raw.len() as u64;
            }
        }
        if !produced {
            thread::sleep(Duration::from_millis(50));
        }
    }
}

pub(super) struct LogStreamer {
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl LogStreamer {
    pub fn start(paths: Vec<PathBuf>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            let mut offsets: Vec<u64> = vec![0; paths.len()];
            let mut shown = HashSet::<String>::new();
            let mut signature_mismatch_seen = false;
            for (i, p) in paths.iter().enumerate() {
                offsets[i] = fs::metadata(p).map(|m| m.len()).unwrap_or(0);
            }
            while !stop_clone.load(Ordering::Relaxed) {
                for (i, p) in paths.iter().enumerate() {
                    let Ok(meta) = fs::metadata(p) else { continue };
                    let len = meta.len();
                    if len <= offsets[i] {
                        if len < offsets[i] {
                            offsets[i] = 0;
                        }
                        continue;
                    }
                    let Ok(raw) = fs::read(p) else { continue };
                    let from = offsets[i] as usize;
                    if from >= raw.len() {
                        continue;
                    }
                    let chunk = &raw[from..];
                    if let Ok(text) = std::str::from_utf8(chunk) {
                        for line in text.lines() {
                            let cleaned = strip_ansi(line);
                            let trimmed = cleaned.trim();
                            if is_signature_mismatch_line(trimmed) {
                                signature_mismatch_seen = true;
                                continue;
                            }
                            if signature_mismatch_seen && is_signature_mismatch_followup(trimmed) {
                                continue;
                            }
                            if let Some(display) = console_log_line(trimmed)
                                && shown.insert(display.clone())
                            {
                                eprintln!("  │ {display}");
                            }
                        }
                        offsets[i] = raw.len() as u64;
                    }
                }
                thread::sleep(Duration::from_millis(400));
            }
        });
        Self {
            stop,
            handle: Some(handle),
        }
    }

    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn console_log_line(line: &str) -> Option<String> {
    let cleaned = strip_ansi(line);
    let trimmed = cleaned.trim();
    if trimmed.len() < 3 || trimmed.starts_with('$') || trimmed.starts_with("---") {
        return None;
    }
    if verbose_android_logs() {
        return Some(trim_log_line(trimmed));
    }
    if is_noise_line(trimmed) {
        return None;
    }
    if let Some(line) = compact_known_line(trimmed) {
        return Some(line);
    }
    if is_error_line(trimmed) {
        return Some(format!(
            "issue: {}",
            trim_log_line(&compact_logcat_line(trimmed))
        ));
    }
    None
}

fn verbose_android_logs() -> bool {
    std::env::var_os("WT_ANDROID_VERBOSE").is_some()
        || std::env::var_os("ANDROID_VERBOSE_LOGS").is_some()
}

fn compact_known_line(line: &str) -> Option<String> {
    if line.contains("VITE v") && line.contains("ready in") {
        return Some(trim_log_line(line));
    }
    if line.contains("Local:") && line.contains(":1420") {
        return Some(trim_log_line(line));
    }
    if line.contains("Network:") && line.contains(":1420") {
        return Some(trim_log_line(line));
    }
    if line.contains("android dev: detected ABI") {
        return Some(trim_log_line(line));
    }
    if line.contains("Using installed NDK") {
        return Some(trim_log_line(line));
    }
    if line.contains("Replacing devUrl host") {
        return Some("tauri dev URL host prepared".to_string());
    }
    if line.trim_start().starts_with("Compiling wtranscriber ") {
        return Some("cargo compiling wtranscriber".to_string());
    }
    if line.trim_start().starts_with("Compiling ") {
        return Some("cargo compiling Rust crates".to_string());
    }
    if line.contains("Finished `dev` profile") {
        return Some(trim_log_line(line));
    }
    if line.contains("Performing Streamed Install") {
        return Some("apk installing".to_string());
    }
    if line.trim() == "Success" {
        return Some("apk install success".to_string());
    }
    if line.contains("Starting: Intent") && line.contains("wtranscriber") {
        return Some("app launch intent sent".to_string());
    }
    if line.contains("Info Opening ") {
        return Some(trim_log_line(line));
    }
    if line.contains("Forwarding port 1420") {
        return Some("adb reverse tcp:1420 ready".to_string());
    }
    if line.contains("Watching ") && line.contains("src-tauri") {
        return Some("watching src-tauri for Rust changes".to_string());
    }
    if let Some(line) = compact_rust_stdout(line) {
        return Some(line);
    }
    if (line.contains("am_crash") || line.contains("am_proc_died") || line.contains("am_kill"))
        && line.contains("wtranscriber")
    {
        return Some(trim_log_line(line));
    }
    None
}

fn compact_rust_stdout(line: &str) -> Option<String> {
    let (_, msg) = line.split_once("RustStdoutStderr:")?;
    let msg = msg.trim();
    if msg.starts_with("INFO") || msg.starts_with("WARN") || msg.starts_with("ERROR") {
        return Some(format!("app: {}", trim_log_line(msg)));
    }
    if msg.contains(" WARN ") || msg.contains(" ERROR ") {
        return Some(format!("app: {}", trim_log_line(msg)));
    }
    None
}

fn is_error_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    (lower.contains("panic")
        || lower.contains(" error")
        || lower.contains("error:")
        || lower.contains("failed")
        || lower.contains("denied")
        || lower.contains("not allowed")
        || lower.contains("install_failed"))
        && !is_noise_line(line)
}

fn is_signature_mismatch_line(line: &str) -> bool {
    line.contains("INSTALL_FAILED_UPDATE_INCOMPATIBLE") || line.contains("signatures do not match")
}

fn is_signature_mismatch_followup(line: &str) -> bool {
    line.contains("error: script \"tauri\" exited with code 1")
        || line.contains("Error: bun [\"run\", \"tauri\", \"android\", \"dev\"")
}

fn is_noise_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    [
        "external vite already running",
        "requires shared lib",
        "symlinking lib",
        "symlink at",
        "tcp:1420 already forwarded",
        "failed to read dnsconfig",
        "unsupported mediatype",
        "unsupported mime",
        "unrecognized profile/level",
        "bluetooth_connect permission is missing",
        "requires bluetooth permission",
        "access denied finding property",
        "pinning is deprecated",
        "couldn't find an opengl es implementation",
        "failed to load pipeline blob cache",
        "failed to open file for reading seed",
        "variations_seed_loader",
        "http cache size is",
        "page_load_metrics_update_dispatcher",
        "invalid first_paint",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn compact_logcat_line(line: &str) -> String {
    if let Some((_, msg)) = line.split_once("RustStdoutStderr:") {
        return msg.trim().to_string();
    }
    line.to_string()
}

fn trim_log_line(line: &str) -> String {
    let no_ansi = strip_ansi(line);
    let trimmed = no_ansi.trim();
    if trimmed.chars().count() > 140 {
        format!("{}…", trimmed.chars().take(139).collect::<String>())
    } else {
        trimmed.to_string()
    }
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for c in chars.by_ref() {
                if ('@'..='~').contains(&c) {
                    break;
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

pub(super) fn install_signature_mismatch(paths: &[&Path]) -> bool {
    paths.iter().any(|p| {
        last_line_matching(p, |s| {
            s.contains("INSTALL_FAILED_UPDATE_INCOMPATIBLE")
                || s.contains("signatures do not match")
        })
        .is_some()
    })
}

pub(super) fn api_probe(timeout: Duration) -> Option<String> {
    let expr = concat!(
        "(async () => {",
        "const invoke = window.__TAURI_INTERNALS__?.invoke;",
        "if (typeof invoke !== 'function') throw new Error('tauri invoke unavailable');",
        "const systemInfo = await invoke('system_info');",
        "return {version: systemInfo.app_version ?? systemInfo.appVersion, ",
        "os: systemInfo.os, ok: true};",
        "})()"
    );
    capture_timeout("bun", &["scripts/cdp.ts", expr], timeout)
}

pub(super) fn read_pids(path: &Path) -> BTreeMap<String, u32> {
    let Ok(raw) = fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(&raw) else {
        return BTreeMap::new();
    };
    value
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(k, v)| Some((k.clone(), u32::try_from(v.as_u64()?).ok()?)))
        .collect()
}

pub(super) fn pids_device(path: &Path) -> Option<String> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str::<Value>(&raw)
        .ok()?
        .get("device")?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}
