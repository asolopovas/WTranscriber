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

const RTF_WINDOW_SIZE: usize = 8;
const RTF_PRIOR_WEIGHT: f64 = 0.7;
const RTF_SAMPLE_ALPHA: f64 = 0.20;
const DISPLAY_MAX_ADVANCE: f64 = 0.10;
const ETA_CORRECTION_ALPHA: f64 = 0.30;

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
    eta_anchor: Instant,
    eta_at_anchor: f64,
    have_eta: bool,
}

fn effective_rtf(rtf: f64, prior: f64) -> f64 {
    if rtf > 0.0 {
        rtf
    } else if prior > 0.0 {
        prior
    } else {
        1.0
    }
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
            eta_anchor: now,
            eta_at_anchor: 0.0,
            have_eta: false,
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

        let frac_remaining = ((100.0 - f64::from(pct)).max(0.0)) / 100.0;
        let rtf = effective_rtf(self.rtf, self.prior_rtf);
        let raw_eta = (self.audio_dur_sec * frac_remaining / rtf).max(0.0);
        if self.have_eta {
            let now2 = Instant::now();
            let elapsed_since_anchor = now2.duration_since(self.eta_anchor).as_secs_f64();
            let projected = (self.eta_at_anchor - elapsed_since_anchor).max(0.0);
            self.eta_at_anchor =
                (1.0 - ETA_CORRECTION_ALPHA) * projected + ETA_CORRECTION_ALPHA * raw_eta;
            self.eta_anchor = now2;
        } else {
            self.eta_at_anchor = raw_eta;
            self.eta_anchor = Instant::now();
            self.have_eta = true;
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

        let eta = if self.have_eta {
            let elapsed_since_anchor = Instant::now().duration_since(self.eta_anchor).as_secs_f64();
            (self.eta_at_anchor - elapsed_since_anchor).max(0.0)
        } else {
            let remaining_audio = (self.audio_dur_sec * (1.0 - display / 100.0)).max(0.0);
            remaining_audio / rtf
        };
        (display, eta)
    }

    #[must_use]
    pub fn elapsed(&self) -> Duration {
        Instant::now().duration_since(self.start_time)
    }

    #[must_use]
    pub const fn observed_rtf(&self) -> f64 {
        self.rtf
    }

    #[must_use]
    pub fn total_wall_sec(&self) -> f64 {
        self.audio_dur_sec / effective_rtf(self.rtf, self.prior_rtf)
    }

    #[must_use]
    pub fn remaining_wall_sec(&self) -> f64 {
        let frac = (100.0 - f64::from(self.last_pct)).max(0.0) / 100.0;
        self.audio_dur_sec * frac / effective_rtf(self.rtf, self.prior_rtf)
    }
}

const DIARIZE_RATE_ALPHA: f64 = 0.4;
const DIARIZE_ETA_ALPHA: f64 = 0.30;
const DIARIZE_PREDICT_DAMP: f64 = 0.5;
pub const DIARIZE_DEFAULT_RTF: f64 = 6.0;

pub struct DiarizeSmoother {
    start: Instant,
    audio_dur_sec: f64,
    prior_rate: f64,
    last_pct: f64,
    last_pct_at: Instant,
    rate: f64,
    rate_init: bool,
    display_shown: f64,
    eta_shown: f64,
    have_eta: bool,
}

impl Default for DiarizeSmoother {
    fn default() -> Self {
        Self::new(1.0, DIARIZE_DEFAULT_RTF)
    }
}

impl DiarizeSmoother {
    #[must_use]
    pub fn new(audio_dur_sec: f64, prior_rtf: f64) -> Self {
        let now = Instant::now();
        let dur = if audio_dur_sec > 0.0 {
            audio_dur_sec
        } else {
            1.0
        };
        let rtf = if prior_rtf > 0.0 {
            prior_rtf
        } else {
            DIARIZE_DEFAULT_RTF
        };
        let prior_rate = (rtf * 100.0 / dur).max(0.05);
        Self {
            start: now,
            audio_dur_sec: dur,
            prior_rate,
            last_pct: 0.0,
            last_pct_at: now,
            rate: 0.0,
            rate_init: false,
            display_shown: 0.0,
            eta_shown: 0.0,
            have_eta: false,
        }
    }

    fn current_rate(&self) -> f64 {
        if self.rate_init && self.rate > 0.0 {
            self.rate
        } else {
            self.prior_rate
        }
    }

    #[must_use]
    pub fn total_wall_sec(&self) -> f64 {
        100.0 / self.current_rate()
    }

    #[must_use]
    pub fn remaining_wall_sec(&self) -> f64 {
        ((100.0 - self.last_pct).max(0.0)) / self.current_rate()
    }

    #[must_use]
    pub fn observed_rtf(&self) -> f64 {
        if self.rate_init && self.rate > 0.0 {
            self.rate * self.audio_dur_sec / 100.0
        } else {
            0.0
        }
    }

    pub fn report(&mut self, pct: f64) {
        let clamped = pct.clamp(0.0, 100.0);
        if clamped <= self.last_pct {
            return;
        }
        let now = Instant::now();
        let dt = now.duration_since(self.last_pct_at).as_secs_f64();
        let dpct = clamped - self.last_pct;
        if dt >= 0.005 && dpct > 0.0 {
            let observed = dpct / dt;
            self.rate = if self.rate_init {
                (1.0 - DIARIZE_RATE_ALPHA) * self.rate + DIARIZE_RATE_ALPHA * observed
            } else {
                observed
            };
            self.rate_init = true;
        }
        self.last_pct = clamped;
        self.last_pct_at = now;
    }

    pub fn snapshot(&mut self) -> (f64, f64) {
        let now = Instant::now();
        let rate = self.current_rate();
        let since_last = now.duration_since(self.last_pct_at).as_secs_f64();
        let predicted = rate * since_last * DIARIZE_PREDICT_DAMP;
        let mut display = self.last_pct + predicted;
        if display > 99.0 {
            display = 99.0;
        }
        if display < self.display_shown {
            display = self.display_shown;
        }
        self.display_shown = display;

        let raw_eta = if rate > 0.0 {
            ((100.0 - display) / rate).max(0.0)
        } else {
            0.0
        };
        if self.have_eta {
            self.eta_shown =
                (1.0 - DIARIZE_ETA_ALPHA) * self.eta_shown + DIARIZE_ETA_ALPHA * raw_eta;
        } else {
            self.eta_shown = raw_eta;
            self.have_eta = true;
        }
        if self.eta_shown < 0.0 {
            self.eta_shown = 0.0;
        }
        (display, self.eta_shown)
    }

    #[must_use]
    pub fn elapsed(&self) -> Duration {
        Instant::now().duration_since(self.start)
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

#[must_use]
pub const fn default_diarize_rtf(_backend: &str) -> f64 {
    DIARIZE_DEFAULT_RTF
}

fn diarize_key(backend: &str) -> String {
    format!("diarize|{}", backend.to_lowercase())
}

#[must_use]
pub fn load_diarize_rtf(backend: &str) -> f64 {
    let _guard = RTF_LOCK.lock().ok();
    let Ok(path) = rtf_path() else {
        return default_diarize_rtf(backend);
    };
    let Ok(data) = std::fs::read_to_string(&path) else {
        return default_diarize_rtf(backend);
    };
    let Ok(map) = serde_json::from_str::<HashMap<String, f64>>(&data) else {
        return default_diarize_rtf(backend);
    };
    map.get(&diarize_key(backend))
        .copied()
        .filter(|v| *v > 0.0)
        .unwrap_or_else(|| default_diarize_rtf(backend))
}

pub fn save_diarize_rtf(backend: &str, observed: f64) {
    if observed <= 0.0 {
        return;
    }
    let _guard = RTF_LOCK.lock().ok();
    let Ok(path) = rtf_path() else { return };
    let mut map: HashMap<String, f64> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|d| serde_json::from_str(&d).ok())
        .unwrap_or_default();
    let key = diarize_key(backend);
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
    fn set_diarize_backend(&self, _name: &str) {}
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

    #[test]
    fn diarize_initial_snapshot_uses_prior_rate() {
        let mut d = DiarizeSmoother::new(60.0, DIARIZE_DEFAULT_RTF);
        let (pct, eta) = d.snapshot();
        assert!(pct < 1e-6);
        assert!(eta > 0.0, "prior ETA should be positive, got {eta}");
        let expected_total = 60.0 / DIARIZE_DEFAULT_RTF;
        assert!(
            eta <= expected_total + 0.5,
            "eta {eta} should be near total {expected_total}"
        );
    }

    #[test]
    fn diarize_prior_scales_with_audio_duration() {
        let short = DiarizeSmoother::new(30.0, DIARIZE_DEFAULT_RTF).total_wall_sec();
        let long = DiarizeSmoother::new(600.0, DIARIZE_DEFAULT_RTF).total_wall_sec();
        assert!(
            long > short * 5.0,
            "long={long} should scale beyond short={short}"
        );
    }

    #[test]
    fn diarize_pct_advances_after_report() {
        let mut d = DiarizeSmoother::new(60.0, DIARIZE_DEFAULT_RTF);
        d.report(25.0);
        let (pct, _) = d.snapshot();
        assert!(pct >= 25.0);
        assert!(pct < 100.0);
    }

    #[test]
    fn diarize_pct_is_monotonic() {
        let mut d = DiarizeSmoother::new(60.0, DIARIZE_DEFAULT_RTF);
        d.report(30.0);
        let (a, _) = d.snapshot();
        let (b, _) = d.snapshot();
        assert!(b >= a);
    }

    #[test]
    fn diarize_eta_decreases_as_pct_grows() {
        let mut d = DiarizeSmoother::new(60.0, DIARIZE_DEFAULT_RTF);
        d.report(20.0);
        let (_, eta_low) = d.snapshot();
        d.report(80.0);
        let (_, eta_high) = d.snapshot();
        assert!(
            eta_high < eta_low,
            "eta should drop: {eta_low} -> {eta_high}"
        );
    }

    #[test]
    fn diarize_ignores_backwards_reports() {
        let mut d = DiarizeSmoother::new(60.0, DIARIZE_DEFAULT_RTF);
        d.report(50.0);
        d.report(20.0);
        let (pct, _) = d.snapshot();
        assert!(pct >= 50.0);
    }

    #[test]
    fn diarize_clamps_below_100() {
        let mut d = DiarizeSmoother::new(60.0, DIARIZE_DEFAULT_RTF);
        d.report(99.5);
        let (pct, _) = d.snapshot();
        assert!(pct <= 99.0 + 1e-9);
    }
}
