#![allow(
    dead_code,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops
)]

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use crate::{error::Result, paths};

const RTF_WINDOW_SIZE: usize = 6;
const RTF_PRIOR_WEIGHT: f64 = 0.7;
const RTF_SAMPLE_ALPHA: f64 = 0.35;
const DISPLAY_MAX_ADVANCE: f64 = 0.10;
const ETA_SMOOTH_ALPHA: f64 = 0.25;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    CacheCheck,
    LoadingAudio,
    Transcribing,
    Diarizing,
    Writing,
    Done,
}

#[derive(Debug, Clone, Copy)]
struct RtfSample {
    pct_delta: f64,
    elapsed: f64,
}

pub struct Smoother {
    audio_dur_sec: f64,
    prior_rtf: f64,
    rtf: f64,
    samples: Vec<RtfSample>,
    total_samples: usize,
    last_pct: i32,
    last_tick: Instant,
    start_time: Instant,
    display_shown: f64,
    eta_shown: f64,
}

impl Smoother {
    #[must_use]
    pub fn new(audio_dur_sec: f64, initial_rtf: f64) -> Self {
        let prior = if initial_rtf > 0.0 { initial_rtf } else { 1.0 };
        let dur = if audio_dur_sec > 0.0 {
            audio_dur_sec
        } else {
            1.0
        };
        let now = Instant::now();
        Self {
            audio_dur_sec: dur,
            prior_rtf: prior,
            rtf: prior,
            samples: Vec::with_capacity(RTF_WINDOW_SIZE),
            total_samples: 0,
            last_pct: 0,
            last_tick: now,
            start_time: now,
            display_shown: 0.0,
            eta_shown: 0.0,
        }
    }

    pub fn report(&mut self, pct: i32) {
        if pct <= self.last_pct {
            return;
        }
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_tick).as_secs_f64();
        let pct_delta = f64::from(pct - self.last_pct);
        self.last_pct = pct;
        self.last_tick = now;

        if elapsed <= 0.0 || pct_delta <= 0.0 {
            return;
        }

        self.samples.push(RtfSample { pct_delta, elapsed });
        if self.samples.len() > RTF_WINDOW_SIZE {
            self.samples.remove(0);
        }
        self.total_samples += 1;

        let mut total_pct = 0.0;
        let mut total_elapsed = 0.0;
        for s in &self.samples {
            total_pct += s.pct_delta;
            total_elapsed += s.elapsed;
        }
        if total_elapsed <= 0.0 {
            return;
        }
        let window_audio = total_pct / 100.0 * self.audio_dur_sec;
        let window_rtf = window_audio / total_elapsed;

        let prior_blend = if self.total_samples >= 3 {
            0.0
        } else if self.total_samples == 2 {
            0.35
        } else {
            RTF_PRIOR_WEIGHT
        };
        let blended = prior_blend * self.prior_rtf + (1.0 - prior_blend) * window_rtf;

        if self.total_samples <= 1 {
            self.rtf = blended;
        } else {
            self.rtf = (1.0 - RTF_SAMPLE_ALPHA) * self.rtf + RTF_SAMPLE_ALPHA * blended;
        }
    }

    pub fn snapshot(&mut self) -> (f64, f64) {
        let mut rtf = self.rtf;
        if rtf <= 0.0 {
            rtf = self.prior_rtf;
        }
        if rtf <= 0.0 {
            rtf = 1.0;
        }

        let elapsed_since_tick = Instant::now().duration_since(self.last_tick).as_secs_f64();
        let mut sec_per_pct = self.audio_dur_sec / 100.0 / rtf;
        if sec_per_pct <= 0.0 {
            sec_per_pct = 0.1;
        }
        let mut predicted = elapsed_since_tick / sec_per_pct;
        let max_jump = DISPLAY_MAX_ADVANCE * 100.0;
        if predicted > max_jump {
            predicted = max_jump;
        }
        let mut display = f64::from(self.last_pct) + predicted;
        if display > 99.0 {
            display = 99.0;
        }
        if display < self.display_shown {
            display = self.display_shown;
        }
        self.display_shown = display;

        let mut remaining_audio = self.audio_dur_sec * (1.0 - display / 100.0);
        if remaining_audio < 0.0 {
            remaining_audio = 0.0;
        }
        let raw_eta = remaining_audio / rtf;
        if self.eta_shown <= 0.0 {
            self.eta_shown = raw_eta;
        } else {
            self.eta_shown = (1.0 - ETA_SMOOTH_ALPHA) * self.eta_shown + ETA_SMOOTH_ALPHA * raw_eta;
        }
        if self.eta_shown < 0.0 {
            self.eta_shown = 0.0;
        }
        (display, self.eta_shown)
    }

    #[must_use]
    pub fn elapsed(&self) -> Duration {
        Instant::now().duration_since(self.start_time)
    }

    #[must_use]
    pub const fn observed_rtf(&self) -> f64 {
        self.rtf
    }
}

#[must_use]
pub fn default_rtf(model: &str, device: &str) -> f64 {
    let m = model.to_lowercase();
    let is_cpu = device.is_empty() || device.to_lowercase().contains("cpu");
    let mut base = if m.starts_with("tiny") {
        3.0
    } else if m.starts_with("base") {
        2.0
    } else if m.starts_with("small") {
        1.0
    } else if m.starts_with("medium") {
        0.5
    } else if m.contains("large-v3-turbo") || m.contains("turbo") {
        0.8
    } else if m.starts_with("large") {
        0.3
    } else {
        1.0
    };
    if is_cpu {
        base /= 3.0;
    }
    if base < 0.05 {
        base = 0.05;
    }
    base
}

static RTF_LOCK: Mutex<()> = Mutex::new(());

fn rtf_path() -> Result<std::path::PathBuf> {
    Ok(paths::config_dir()?.join("rtf.json"))
}

fn rtf_key(model: &str, device: &str) -> String {
    format!("{}|{}", model.to_lowercase(), device.to_lowercase())
}

#[must_use]
pub fn load_rtf(model: &str, device: &str) -> f64 {
    let _guard = RTF_LOCK.lock().ok();
    let Ok(path) = rtf_path() else {
        return default_rtf(model, device);
    };
    let Ok(data) = std::fs::read_to_string(&path) else {
        return default_rtf(model, device);
    };
    let Ok(map) = serde_json::from_str::<HashMap<String, f64>>(&data) else {
        return default_rtf(model, device);
    };
    map.get(&rtf_key(model, device))
        .copied()
        .filter(|v| *v > 0.0)
        .unwrap_or_else(|| default_rtf(model, device))
}

pub fn save_rtf(model: &str, device: &str, observed: f64) {
    if observed <= 0.0 {
        return;
    }
    let _guard = RTF_LOCK.lock().ok();
    let Ok(path) = rtf_path() else { return };
    let mut map: HashMap<String, f64> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|d| serde_json::from_str(&d).ok())
        .unwrap_or_default();
    let key = rtf_key(model, device);
    let blended = match map.get(&key).copied() {
        Some(prev) if prev > 0.0 => 0.5 * prev + 0.5 * observed,
        _ => observed,
    };
    map.insert(key, blended);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(data) = serde_json::to_string_pretty(&map) {
        let _ = std::fs::write(&path, data);
    }
}

pub trait Sink: Send + Sync {
    fn phase(&self, phase: Phase);
    fn report_pct(&self, phase: Phase, pct: f64);
    fn is_cancelled(&self) -> bool {
        false
    }
}

pub struct NoopSink;
impl Sink for NoopSink {
    fn phase(&self, _: Phase) {}
    fn report_pct(&self, _: Phase, _: f64) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eta_non_negative() {
        let mut s = Smoother::new(60.0, 1.0);
        s.report(50);
        let (_, eta) = s.snapshot();
        assert!(eta >= 0.0);
    }

    #[test]
    fn default_rtf_cpu_lower_than_gpu() {
        let cpu = default_rtf("large-v3-turbo", "cpu");
        let gpu = default_rtf("large-v3-turbo", "cuda");
        assert!(cpu < gpu);
    }
}
