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
