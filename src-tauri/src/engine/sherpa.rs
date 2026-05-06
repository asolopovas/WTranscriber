use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};

use serde::Deserialize;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Default, Deserialize)]
#[allow(dead_code)]
pub struct SherpaResult {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub tokens: Vec<String>,
    #[serde(default)]
    pub timestamps: Vec<f64>,
    #[serde(default)]
    pub lang: String,
    #[serde(default)]
    pub emotion: String,
    #[serde(default)]
    pub event: String,
}

pub const fn binary_name() -> &'static str {
    if cfg!(windows) {
        "sherpa-onnx-offline.exe"
    } else {
        "sherpa-onnx-offline"
    }
}

pub fn find_binary() -> Result<PathBuf> {
    let name = binary_name();

    if let Ok(env_dir) = std::env::var("WT_SHERPA_DIR") {
        let p = Path::new(&env_dir).join(name);
        if p.exists() {
            return Ok(p);
        }
    }

    if let Some(p) = crate::runtimes::sherpa::find_any(name) {
        return Ok(p);
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

    Err(Error::Transcribe(format!(
        "{name} not found (set WT_SHERPA_DIR or install sherpa-onnx)"
    )))
}

pub fn run_cmd(bin: &Path, args: &[String]) -> Result<(String, String, f64)> {
    let start = Instant::now();
    let mut cmd = build_command(bin);
    cmd.args(args);
    let out = cmd.output()?;
    let elapsed = start.elapsed().as_secs_f64();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    if !out.status.success() {
        return Err(Error::Transcribe(format!(
            "sherpa subprocess failed: {}",
            stderr.trim()
        )));
    }
    Ok((stdout, stderr, elapsed))
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

pub fn parse_json(stdout: &str) -> Result<SherpaResult> {
    for line in stdout.lines() {
        let line = line.trim();
        if !line.starts_with('{') || !line.contains("\"text\"") {
            continue;
        }
        let Ok(r) = serde_json::from_str::<SherpaResult>(line) else {
            continue;
        };
        if r.text.trim().is_empty() {
            return Err(Error::Transcribe("empty transcript".into()));
        }
        return Ok(r);
    }
    Err(Error::Transcribe(
        "no JSON result line in subprocess output".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_json_line() {
        let out = "noise\n{\"text\":\"hello world\",\"tokens\":[\" hello\",\" world\"],\"timestamps\":[0.1,0.5]}\n";
        let r = parse_json(out).unwrap();
        assert_eq!(r.text, "hello world");
        assert_eq!(r.tokens.len(), 2);
        assert_eq!(r.timestamps, vec![0.1, 0.5]);
    }

    #[test]
    fn rejects_missing_json() {
        assert!(parse_json("garbage").is_err());
    }

    #[test]
    fn rejects_empty_text() {
        let out = "{\"text\":\"\"}";
        assert!(parse_json(out).is_err());
    }
}
