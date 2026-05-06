#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::LazyLock,
};

use regex::Regex;

use crate::{
    diarizer::{Backend, Progress, Segment},
    error::{Error, Result},
    paths,
};

const SEG_REL: &str = "sherpa-onnx-pyannote-segmentation-3-0/model.onnx";
const EMB_REL: &str = "titanet_large.onnx";

static SEG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*([0-9]+\.[0-9]+)\s+--\s+([0-9]+\.[0-9]+)\s+speaker_(\d+)\s*$").unwrap()
});

static PROG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"progress\s+([0-9]+\.[0-9]+)%").unwrap());

#[derive(Debug, Clone)]
pub struct SherpaDiarizer {
    bin: PathBuf,
    seg_model: PathBuf,
    emb_model: PathBuf,
    num_speakers: u32,
}

const fn bin_name() -> &'static str {
    if cfg!(windows) {
        "sherpa-onnx-offline-speaker-diarization.exe"
    } else {
        "sherpa-onnx-offline-speaker-diarization"
    }
}

fn find_bin() -> Result<PathBuf> {
    let name = bin_name();
    if let Ok(env_dir) = std::env::var("WT_SHERPA_DIR") {
        let p = Path::new(&env_dir).join(name);
        if p.exists() {
            return Ok(p);
        }
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let p = dir.join(name);
        if p.exists() {
            return Ok(p);
        }
    }
    if let Ok(p) = which::which(name) {
        return Ok(p);
    }
    Err(Error::Transcribe(format!("{name} not found")))
}

fn resolve_models() -> Result<(PathBuf, PathBuf)> {
    let root = paths::models_dir()?;
    let seg = root.join(SEG_REL.replace('/', std::path::MAIN_SEPARATOR_STR));
    let emb = root.join(EMB_REL);
    if !seg.exists() {
        return Err(Error::Transcribe(format!(
            "diarizer segmentation model missing at {}",
            seg.display()
        )));
    }
    if !emb.exists() {
        return Err(Error::Transcribe(format!(
            "diarizer embedding model missing at {}",
            emb.display()
        )));
    }
    Ok((seg, emb))
}

fn diarizer_threads() -> u32 {
    let n = std::thread::available_parallelism().map_or(4, std::num::NonZero::get) / 2;
    u32::try_from(n).unwrap_or(4).clamp(2, 8)
}

impl SherpaDiarizer {
    pub fn new(num_speakers: u32) -> Result<Self> {
        let bin = find_bin()?;
        let (seg_model, emb_model) = resolve_models()?;
        Ok(Self {
            bin,
            seg_model,
            emb_model,
            num_speakers,
        })
    }

    fn args(&self, wav: &Path) -> Vec<String> {
        let threads = diarizer_threads();
        let mut a = vec![
            format!("--segmentation.pyannote-model={}", self.seg_model.display()),
            format!("--embedding.model={}", self.emb_model.display()),
            format!("--segmentation.num-threads={threads}"),
            format!("--embedding.num-threads={threads}"),
            "--min-duration-on=0.2".into(),
            "--min-duration-off=0.2".into(),
        ];
        if self.num_speakers > 0 {
            a.push(format!("--clustering.num-clusters={}", self.num_speakers));
        } else {
            a.push("--clustering.cluster-threshold=0.75".into());
        }
        a.push(wav.display().to_string());
        a
    }
}

impl Backend for SherpaDiarizer {
    fn name(&self) -> String {
        format!(
            "sherpa-onnx-pyannote+{}",
            self.emb_model
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
        )
    }

    fn diarize(
        &self,
        wav: &Path,
        _num_speakers: u32,
        _audio_dur_sec: f64,
        on_progress: Progress<'_>,
    ) -> Result<Vec<Segment>> {
        let mut cmd = build_command(&self.bin);
        cmd.args(self.args(wav))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Transcribe("no stdout from diarizer".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| Error::Transcribe("no stderr from diarizer".into()))?;

        let stderr_handle = std::thread::spawn({
            let mut last_pct = 0.0_f64;
            move || {
                let mut buf = String::new();
                let mut lines = Vec::new();
                let r = BufReader::new(stderr);
                for line in r.lines().map_while(std::result::Result::ok) {
                    if let Some(caps) = PROG_RE.captures(&line)
                        && let Ok(p) = caps[1].parse::<f64>()
                        && p > last_pct
                    {
                        last_pct = p.min(99.0);
                        lines.push(last_pct);
                    }
                    buf.push_str(&line);
                    buf.push('\n');
                }
                (buf, lines)
            }
        });

        let mut segments = Vec::new();
        for line in BufReader::new(stdout)
            .lines()
            .map_while(std::result::Result::ok)
        {
            let line = line.trim_end_matches('\r').to_string();
            if let Some(caps) = SEG_RE.captures(&line) {
                let start: f64 = caps[1].parse().unwrap_or(0.0);
                let end: f64 = caps[2].parse().unwrap_or(0.0);
                let spk: u32 = caps[3].parse().unwrap_or(0);
                segments.push(Segment {
                    speaker: spk,
                    start_sec: start,
                    end_sec: end,
                });
            }
        }

        let status = child.wait()?;
        let (stderr_buf, progress_pcts) = stderr_handle
            .join()
            .map_err(|_| Error::Transcribe("stderr reader panicked".into()))?;

        for p in progress_pcts {
            on_progress(p);
        }

        if !status.success() {
            return Err(Error::Transcribe(format!(
                "diarizer failed: {}",
                stderr_buf.trim()
            )));
        }

        on_progress(100.0);
        Ok(segments)
    }
}

#[cfg(windows)]
fn build_command(bin: &Path) -> Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut cmd = Command::new(bin);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[cfg(not(windows))]
fn build_command(bin: &Path) -> Command {
    Command::new(bin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_regex_matches() {
        let line = "  0.123 -- 4.567 speaker_2";
        let caps = SEG_RE.captures(line).unwrap();
        assert_eq!(&caps[1], "0.123");
        assert_eq!(&caps[2], "4.567");
        assert_eq!(&caps[3], "2");
    }

    #[test]
    fn progress_regex_matches() {
        let line = "...progress 42.5% done";
        let caps = PROG_RE.captures(line).unwrap();
        assert_eq!(&caps[1], "42.5");
    }
}
