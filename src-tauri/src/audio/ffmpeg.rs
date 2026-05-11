#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

use crate::{
    error::{Error, Result},
    process::quiet_command,
};

static FFMPEG: OnceLock<Option<PathBuf>> = OnceLock::new();
static FFPROBE: OnceLock<Option<PathBuf>> = OnceLock::new();

pub fn find_ffmpeg() -> Option<PathBuf> {
    FFMPEG
        .get_or_init(|| which::which("ffmpeg").ok().or_else(find_ffmpeg_windows))
        .clone()
}

pub fn find_ffprobe() -> Option<PathBuf> {
    FFPROBE
        .get_or_init(|| {
            which::which("ffprobe").ok().or_else(|| {
                let ff = find_ffmpeg()?;
                let mut probe = ff.with_file_name("ffprobe");
                if cfg!(windows) {
                    probe.set_extension("exe");
                }
                probe.exists().then_some(probe)
            })
        })
        .clone()
}

#[cfg(not(windows))]
#[allow(clippy::missing_const_for_fn)]
fn find_ffmpeg_windows() -> Option<PathBuf> {
    None
}

#[cfg(windows)]
fn find_ffmpeg_windows() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    let user = std::env::var_os("USERPROFILE");
    let local = PathBuf::from(local);

    let mut candidates = vec![local.join(r"Microsoft\WinGet\Links\ffmpeg.exe")];

    let pkgs = local.join(r"Microsoft\WinGet\Packages");
    if let Ok(entries) = std::fs::read_dir(&pkgs) {
        for e in entries.flatten() {
            if e.file_name().to_string_lossy().contains("FFmpeg")
                && let Ok(subs) = std::fs::read_dir(e.path())
            {
                for s in subs.flatten() {
                    let n = s.file_name();
                    if s.file_type().is_ok_and(|t| t.is_dir())
                        && n.to_string_lossy().starts_with("ffmpeg")
                    {
                        candidates.push(s.path().join("bin").join("ffmpeg.exe"));
                    }
                }
            }
        }
    }
    if let Some(u) = user {
        candidates.push(PathBuf::from(&u).join(r"scoop\shims\ffmpeg.exe"));
    }
    candidates.push(PathBuf::from(r"C:\ProgramData\chocolatey\bin\ffmpeg.exe"));

    candidates.into_iter().find(|p| p.exists())
}

pub fn probe_duration_ms(path: &Path) -> Option<u64> {
    if let Some(probe) = find_ffprobe() {
        let out = quiet_command(probe.as_os_str())
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=noprint_wrappers=1:nokey=1",
            ])
            .arg(path)
            .output()
            .ok()?;
        if out.status.success()
            && let Ok(s) = std::str::from_utf8(&out.stdout)
            && let Ok(sec) = s.trim().parse::<f64>()
            && sec > 0.0
        {
            return Some((sec * 1000.0) as u64);
        }
    }
    let ffmpeg = find_ffmpeg()?;
    let out = quiet_command(ffmpeg.as_os_str())
        .arg("-i")
        .arg(path)
        .output()
        .ok()?;
    parse_ffmpeg_duration(&String::from_utf8_lossy(&out.stderr))
}

fn parse_ffmpeg_duration(stderr: &str) -> Option<u64> {
    let after = stderr.split_once("Duration:")?.1;
    let hms = after.split_once(',')?.0.trim();
    let mut parts = hms.split(':');
    let h: u64 = parts.next()?.parse().ok()?;
    let m: u64 = parts.next()?.parse().ok()?;
    let s: f64 = parts.next()?.parse().ok()?;
    let total = (h * 3600 + m * 60) as f64 + s;
    (total > 0.0).then_some((total * 1000.0) as u64)
}

pub fn apply_trim(input: &Path, start_ms: u64, end_ms: Option<u64>) -> Result<()> {
    let ffmpeg = find_ffmpeg().ok_or_else(|| {
        Error::Transcribe(
            "ffmpeg not found on PATH; install it (e.g. `sudo apt install ffmpeg`)".into(),
        )
    })?;
    if let Some(end) = end_ms
        && end <= start_ms
    {
        return Err(Error::Transcribe(format!(
            "trim end ({end} ms) must be greater than start ({start_ms} ms)"
        )));
    }
    let ext = input.extension().and_then(|s| s.to_str()).unwrap_or("wav");
    let mut tmp_name = input
        .file_stem()
        .map(std::ffi::OsString::from)
        .unwrap_or_default();
    tmp_name.push(format!(".trim-tmp.{ext}"));
    let tmp = input.with_file_name(tmp_name);
    if tmp.exists() {
        let _ = std::fs::remove_file(&tmp);
    }
    let start_seconds = start_ms as f64 / 1000.0;
    let mut cmd = quiet_command(ffmpeg.as_os_str());
    cmd.args(["-loglevel", "error", "-y", "-i"]).arg(input);
    cmd.args(["-ss", &format!("{start_seconds:.3}")]);
    if let Some(end) = end_ms {
        let end_seconds = end as f64 / 1000.0;
        cmd.args(["-to", &format!("{end_seconds:.3}")]);
    }
    cmd.args(["-c", "copy", "-avoid_negative_ts", "make_zero"])
        .arg(&tmp);
    let out = cmd.output()?;
    if !out.status.success() {
        let _ = std::fs::remove_file(&tmp);
        let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(Error::Transcribe(format!("ffmpeg trim failed: {msg}")));
    }
    if let Err(e) = std::fs::rename(&tmp, input) {
        let _ = std::fs::remove_file(&tmp);
        return Err(Error::Transcribe(format!("replace original failed: {e}")));
    }
    Ok(())
}

pub fn run(ffmpeg: &Path, input: &Path, output: &Path) -> Result<()> {
    let out = quiet_command(ffmpeg.as_os_str())
        .args(["-loglevel", "error", "-y", "-i"])
        .arg(input)
        .args([
            "-ar",
            "16000",
            "-ac",
            "1",
            "-sample_fmt",
            "s16",
            "-f",
            "wav",
        ])
        .arg(output)
        .output()?;
    if !out.status.success() {
        let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(Error::Transcribe(format!("ffmpeg failed: {msg}")));
    }
    Ok(())
}
