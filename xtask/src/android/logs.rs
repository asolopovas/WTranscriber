use anyhow::{Result, bail};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use super::ANDROID_PACKAGE;
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
    timeout: Duration,
    guard: impl Fn() -> bool,
) -> Result<()> {
    let start = Instant::now();
    let deadline = start + timeout;
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
        if now >= deadline {
            break Err(());
        }
        let slice = (deadline - now).min(Duration::from_secs(1));
        match rx.recv_timeout(slice) {
            Ok(line) => {
                if f(&line) {
                    eprintln!("  ✓ {label} ({}s)", start.elapsed().as_secs());
                    break Ok(());
                }
                if last_progress.elapsed() >= Duration::from_secs(5) && is_progress_line(&line) {
                    if last_line.as_deref() != Some(line.as_str()) {
                        eprintln!("  [{:>3}s] {}", start.elapsed().as_secs(), trim_log_line(&line));
                        last_line = Some(line);
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
        Err(()) if !guard() => bail!(
            "{label} aborted — child process exited; see {paths:?} for details"
        ),
        Err(()) => bail!(
            "{label} not seen in {paths:?} within {}s — check adb reverse / TAURI_DEV_HOST / device app launch",
            timeout.as_secs()
        ),
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

fn is_progress_line(line: &str) -> bool {
    let l = line.trim_start();
    !l.starts_with('$') && !l.starts_with("---") && l.len() >= 3
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
                            let trimmed = cleaned.trim_end();
                            if trimmed.is_empty() {
                                continue;
                            }
                            if should_show_line(trimmed) {
                                eprintln!("  │ {}", trim_log_line(trimmed));
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

fn should_show_line(line: &str) -> bool {
    let l = line.trim_start();
    if l.starts_with('$') {
        return false;
    }
    if l.contains("---") && l.starts_with("---") {
        return false;
    }
    if l.len() < 3 {
        return false;
    }
    true
}

fn trim_log_line(line: &str) -> String {
    let no_ansi = strip_ansi(line);
    let trimmed = no_ansi.trim();
    if trimmed.len() > 140 {
        format!("{}…", &trimmed[..139])
    } else {
        trimmed.to_string()
    }
}

fn strip_ansi(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            i += 2;
            while i < bytes.len() && !(0x40..=0x7e).contains(&bytes[i]) {
                i += 1;
            }
            i += 1;
        } else {
            out.push(bytes[i] as char);
            i += 1;
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

pub(super) fn file_age_seconds(path: &Path) -> Option<u64> {
    Some(
        fs::metadata(path)
            .ok()?
            .modified()
            .ok()?
            .elapsed()
            .ok()?
            .as_secs(),
    )
}

pub(super) fn json_seconds(value: &Value) -> String {
    value.as_u64().map_or("-".into(), |v| v.to_string())
}

pub(super) fn api_probe(timeout: Duration) -> Option<String> {
    let expr = concat!(
        "import('/src/api.ts').then(m => Promise.all([",
        "m.api.appVersion(), m.api.systemInfo(), m.api.loadConfig()",
        "]).then(([version, systemInfo]) => ({version, os: systemInfo.os, ok: true})))"
    );
    capture_timeout("bun", &["scripts/cdp.ts", expr], timeout)
}

pub(super) fn is_app_crash_signal(line: &str) -> bool {
    line.contains(ANDROID_PACKAGE)
        && (line.contains("am_crash")
            || line.contains("am_proc_died")
            || (line.contains("am_kill")
                && !line.contains("installPackageLI")
                && !line.contains("due to install")))
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
