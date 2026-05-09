use anyhow::{Result, bail};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
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

pub(super) fn wait_for_log_line(
    paths: &[&Path],
    label: &str,
    f: impl Fn(&str) -> bool,
    timeout: Duration,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if tail_any(paths, &f) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(500));
    }
    bail!(
        "{label} not seen in {paths:?} within {}s — check adb reverse / TAURI_DEV_HOST / device app launch",
        timeout.as_secs()
    )
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
    capture_timeout("node", &["scripts/cdp.mjs", expr], timeout)
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
