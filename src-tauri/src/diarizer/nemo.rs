#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use serde::Deserialize;

use crate::{
    diarizer::{Backend, Progress, Segment},
    error::{Error, Result},
    paths,
    process::quiet_command,
};

#[derive(Debug, Clone)]
pub struct NemoDiarizer {
    python: PathBuf,
    script: PathBuf,
}

#[derive(Debug, Deserialize)]
struct JsonSegment {
    start: f64,
    end: f64,
    speaker: String,
}

impl NemoDiarizer {
    pub fn new() -> Result<Self> {
        Ok(Self {
            python: resolve_python()?,
            script: resolve_script()?,
        })
    }
}

impl Backend for NemoDiarizer {
    fn name(&self) -> String {
        "nemo-sortformer".into()
    }

    fn diarize(
        &self,
        wav: &Path,
        num_speakers: u32,
        audio_dur_sec: f64,
        cancelled: &dyn Fn() -> bool,
        on_progress: Progress<'_>,
    ) -> Result<Vec<Segment>> {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        let mut args = vec![self.script.display().to_string()];
        if num_speakers > 0 {
            args.push("--num-speakers".into());
            args.push(num_speakers.to_string());
        }
        args.push(wav.display().to_string());

        let mut cmd = quiet_command(self.python.as_os_str());
        cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = cmd.spawn()?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Transcribe("no stdout from nemo diarizer".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| Error::Transcribe("no stderr from nemo diarizer".into()))?;

        let done = Arc::new(AtomicBool::new(false));
        let done_for_stderr = done.clone();
        let stderr_handle = std::thread::spawn(move || {
            let mut buf = String::new();
            for line in BufReader::new(stderr)
                .lines()
                .map_while(std::result::Result::ok)
            {
                if line.starts_with("done:") {
                    done_for_stderr.store(true, Ordering::SeqCst);
                }
                buf.push_str(&line);
                buf.push('\n');
            }
            buf
        });

        let start = Instant::now();
        let mut last_pct = 0.0_f64;
        let status = loop {
            if cancelled() {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::Cancelled);
            }
            if let Some(status) = child.try_wait()? {
                break status;
            }
            last_pct = report_time_progress(
                start,
                done.load(Ordering::SeqCst),
                audio_dur_sec,
                last_pct,
                on_progress,
            );
            std::thread::sleep(Duration::from_millis(100));
        };

        let mut raw = Vec::new();
        stdout.read_to_end(&mut raw)?;
        let stderr_buf = stderr_handle
            .join()
            .map_err(|_| Error::Transcribe("stderr reader panicked".into()))?;

        if !status.success() {
            return Err(Error::Transcribe(format!(
                "nemo diarizer failed: {}",
                stderr_buf.trim()
            )));
        }

        let parsed: Vec<JsonSegment> = serde_json::from_slice(&raw)?;
        on_progress(100.0);
        Ok(map_segments(parsed))
    }
}

fn report_time_progress(
    start: Instant,
    done: bool,
    audio_dur_sec: f64,
    last_pct: f64,
    on_progress: Progress<'_>,
) -> f64 {
    let pct = if done {
        95.0
    } else {
        let est_total = (audio_dur_sec * 0.15).max(3.0);
        (start.elapsed().as_secs_f64() / est_total * 90.0).min(90.0)
    };
    if pct > last_pct {
        on_progress(pct);
        pct
    } else {
        last_pct
    }
}

fn map_segments(parsed: Vec<JsonSegment>) -> Vec<Segment> {
    let mut speakers = HashMap::<String, u32>::new();
    let mut next = 0_u32;
    parsed
        .into_iter()
        .map(|seg| {
            let speaker = *speakers.entry(seg.speaker).or_insert_with(|| {
                let id = next;
                next += 1;
                id
            });
            Segment {
                speaker,
                start_sec: seg.start,
                end_sec: seg.end,
            }
        })
        .collect()
}

fn resolve_python() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("WT_PYTHON") {
        let path = PathBuf::from(p);
        if path.exists() {
            return Ok(path);
        }
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let bundled_roots = [
            dir.join("_up_").join("resources").join("nemo-runtime"),
            dir.join("..")
                .join("lib")
                .join("WTranscriber")
                .join("_up_")
                .join("resources")
                .join("nemo-runtime"),
            dir.join("resources").join("nemo-runtime"),
            dir.join("..").join("Resources").join("nemo-runtime"),
        ];
        for root in bundled_roots {
            push_runtime_python(&mut candidates, &root);
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        push_runtime_python(
            &mut candidates,
            &cwd.join("src-tauri").join("resources").join("nemo-runtime"),
        );
        push_runtime_python(&mut candidates, &cwd.join("resources").join("nemo-runtime"));
    }
    let data = paths::data_dir()?;
    candidates.extend([
        data.join("python").join("Scripts").join("python.exe"),
        data.join("python").join("bin").join("python"),
    ]);
    for candidate in &candidates {
        if candidate.exists() {
            return Ok(candidate.clone());
        }
    }
    which::which("python")
        .or_else(|_| which::which("python3"))
        .map_err(|_| Error::Config("python not found for NeMo Sortformer diarization".into()))
}

fn push_runtime_python(candidates: &mut Vec<PathBuf>, root: &std::path::Path) {
    if !root.exists() {
        return;
    }
    let python_root = root.join("python");
    candidates.push(python_root.join("bin").join("python3.12"));
    candidates.push(python_root.join("bin").join("python3"));
    candidates.push(python_root.join("bin").join("python"));
    candidates.push(python_root.join("Scripts").join("python.exe"));
    candidates.push(root.join("bin").join("python3.12"));
    candidates.push(root.join("bin").join("python3"));
    candidates.push(root.join("bin").join("python"));
    candidates.push(root.join("Scripts").join("python.exe"));
}

fn resolve_script() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("WT_NEMO_DIARIZE_SCRIPT") {
        let path = PathBuf::from(p);
        if path.exists() {
            return Ok(path);
        }
    }
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join("diarize.py"));
        candidates.push(dir.join("resources").join("diarize.py"));
        candidates.push(dir.join("_up_").join("scripts").join("diarize.py"));
        candidates.push(
            dir.join("..")
                .join("lib")
                .join("WTranscriber")
                .join("_up_")
                .join("scripts")
                .join("diarize.py"),
        );
        candidates.push(dir.join("..").join("Resources").join("diarize.py"));
        candidates.push(
            dir.join("..")
                .join("Resources")
                .join("_up_")
                .join("scripts")
                .join("diarize.py"),
        );
    }
    candidates.push(paths::data_dir()?.join("diarize.py"));
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("scripts").join("diarize.py"));
        candidates.push(cwd.join("..").join("scripts").join("diarize.py"));
    }
    candidates
        .into_iter()
        .find(|p| p.exists())
        .ok_or_else(|| Error::Config("diarize.py not found for NeMo Sortformer diarization".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(start: f64, end: f64, speaker: &str) -> JsonSegment {
        JsonSegment {
            start,
            end,
            speaker: speaker.into(),
        }
    }

    #[test]
    fn maps_string_speakers_to_stable_numeric_ids() {
        let out = map_segments(vec![
            json(0.0, 1.0, "SPEAKER_01"),
            json(1.0, 2.0, "SPEAKER_02"),
            json(2.0, 3.0, "SPEAKER_01"),
        ]);
        assert_eq!(out[0].speaker, 0);
        assert_eq!(out[1].speaker, 1);
        assert_eq!(out[2].speaker, 0);
    }

    #[test]
    fn map_segments_preserves_timing() {
        let out = map_segments(vec![json(1.5, 2.25, "A")]);
        assert!((out[0].start_sec - 1.5).abs() < 1e-9);
        assert!((out[0].end_sec - 2.25).abs() < 1e-9);
    }

    #[test]
    fn map_segments_empty_input_yields_empty() {
        assert!(map_segments(Vec::new()).is_empty());
    }

    #[test]
    fn time_progress_is_monotonic() {
        let start = Instant::now();
        let pct1 = report_time_progress(start, false, 60.0, 0.0, &mut |_| {});
        let pct2 = report_time_progress(start, false, 60.0, pct1, &mut |_| {});
        assert!(pct2 >= pct1);
    }

    #[test]
    fn time_progress_jumps_to_high_percent_on_completion() {
        let pct = report_time_progress(Instant::now(), true, 60.0, 0.0, &mut |_| {});
        assert!((pct - 95.0).abs() < 1e-9);
    }
}
