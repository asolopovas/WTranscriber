#![allow(clippy::cast_possible_truncation)]

use serde::Serialize;

use crate::paths;

#[tauri::command]
pub const fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[derive(Serialize)]
pub struct SystemInfo {
    pub os: &'static str,
    pub arch: &'static str,
    pub cpu_threads: u32,
    pub is_mobile: bool,
    pub cuda_available: bool,
    pub nnapi_available: bool,
    pub app_version: &'static str,
    pub workdir: Option<String>,
    pub models_dir: Option<String>,
    pub cache_dir: Option<String>,
    pub config_dir: Option<String>,
    pub total_memory_bytes: u64,
}

#[tauri::command]
pub fn system_info() -> SystemInfo {
    let os = std::env::consts::OS;
    let is_mobile = matches!(os, "android" | "ios");
    let cpu_threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get) as u32;
    let cuda_available = !is_mobile && cfg!(feature = "cuda");
    let nnapi_available = os == "android";
    SystemInfo {
        os,
        arch: std::env::consts::ARCH,
        cpu_threads,
        is_mobile,
        cuda_available,
        nnapi_available,
        app_version: env!("CARGO_PKG_VERSION"),
        workdir: paths::default_workdir_override().map(|p| p.display().to_string()),
        models_dir: paths::models_dir().ok().map(|p| p.display().to_string()),
        cache_dir: paths::cache_dir().ok().map(|p| p.display().to_string()),
        config_dir: paths::config_file()
            .ok()
            .and_then(|p| p.parent().map(|d| d.display().to_string())),
        total_memory_bytes: read_total_memory(),
    }
}

fn read_total_memory() -> u64 {
    let Ok(s) = std::fs::read_to_string("/proc/meminfo") else {
        return 0;
    };
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            let kb: u64 = rest
                .split_whitespace()
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            return kb * 1024;
        }
    }
    0
}
