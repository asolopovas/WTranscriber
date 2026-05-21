use std::path::Path;

use crate::{audio, logfile, transcriber::format::format_hms};

pub(super) struct TrimWindow {
    pub(super) start_ms: u64,
    pub(super) end_ms_opt: Option<u64>,
    pub(super) trimmed_dur_sec: f64,
    pub(super) total_dur_ms: u64,
}

pub(super) fn compute_trim_window(input: &Path, trim: &audio::meta::AudioMeta) -> TrimWindow {
    let probe_t0 = std::time::Instant::now();
    let total_dur_ms = audio::probe_duration_ms(input).unwrap_or(0);
    logfile::info(&format!(
        "audio probe: {} dur={} (probed in {:.2}s)",
        input.display(),
        format_hms(std::time::Duration::from_millis(total_dur_ms)),
        probe_t0.elapsed().as_secs_f64(),
    ));
    let start_ms = trim.trim_start_ms.min(total_dur_ms.max(trim.trim_start_ms));
    let end_ms_opt = trim.trim_end_ms.map(|e| {
        if total_dur_ms > 0 {
            e.min(total_dur_ms)
        } else {
            e
        }
    });
    let trimmed_dur_ms = end_ms_opt
        .map(|e| e.saturating_sub(start_ms))
        .or_else(|| (total_dur_ms > 0).then(|| total_dur_ms.saturating_sub(start_ms)));
    let trimmed_dur_sec = trimmed_dur_ms.map_or(0.0, |ms| ms as f64 / 1000.0);
    if start_ms > 0 || end_ms_opt.is_some() {
        logfile::info(&format!(
            "trim active: {}-{} (slice {})",
            format_hms(std::time::Duration::from_millis(start_ms)),
            end_ms_opt.map_or_else(
                || "end".into(),
                |e| format_hms(std::time::Duration::from_millis(e)),
            ),
            format_hms(std::time::Duration::from_secs_f64(trimmed_dur_sec)),
        ));
    }
    TrimWindow {
        start_ms,
        end_ms_opt,
        trimmed_dur_sec,
        total_dur_ms,
    }
}
