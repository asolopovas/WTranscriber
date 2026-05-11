use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

static CUDA_DISABLED: AtomicBool = AtomicBool::new(false);

use serde::Deserialize;

use crate::{
    error::{Error, Result},
    process::{find_executable, quiet_command},
};

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SherpaResult {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub tokens: Vec<String>,
    #[serde(default)]
    pub timestamps: Vec<f64>,
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
    find_executable("WT_SHERPA_DIR", name, || {
        crate::runtimes::sherpa::find_any(name)
    })
    .map_err(|_| {
        Error::Transcribe(format!(
            "{name} not found (set WT_SHERPA_DIR or install sherpa-onnx)"
        ))
    })
}

pub fn run_cmd(
    bin: &Path,
    args: &[String],
    cancelled: &dyn Fn() -> bool,
) -> Result<(String, String, f64)> {
    let start = Instant::now();
    let effective: Vec<String> = if CUDA_DISABLED.load(Ordering::Relaxed) && uses_cuda(args) {
        swap_provider_to_cpu(args)
    } else {
        args.to_vec()
    };
    let provider = if uses_cuda(&effective) { "cuda" } else { "cpu" };
    let wav_arg = effective.last().cloned().unwrap_or_default();
    crate::logfile::info(&format!("sherpa spawn: provider={provider} wav={wav_arg}"));
    let out = exec(bin, &effective, cancelled)?;
    crate::logfile::info(&format!(
        "sherpa exit: provider={provider} elapsed={:.2}s status={}",
        start.elapsed().as_secs_f64(),
        if out.status.success() { "ok" } else { "fail" },
    ));
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    if out.status.success() {
        return Ok((stdout, stderr, start.elapsed().as_secs_f64()));
    }
    crate::logfile::warn(&format!(
        "sherpa stderr ({} bytes): {}",
        stderr.len(),
        truncate_for_log(&stderr, 800),
    ));
    if uses_cuda(&effective) && is_cuda_load_failure(&stderr) {
        if !CUDA_DISABLED.swap(true, Ordering::Relaxed) {
            crate::logfile::warn(&format!(
                "sherpa CUDA provider unavailable ({}); falling back to CPU. \
                 To enable GPU acceleration run `just cudnn` (Windows) or install cuDNN 9.x for CUDA 12.x.",
                cuda_failure_reason(&stderr)
            ));
        }
        let cpu_args = swap_provider_to_cpu(&effective);
        let out2 = exec(bin, &cpu_args, cancelled)?;
        let stdout2 = String::from_utf8_lossy(&out2.stdout).into_owned();
        let stderr2 = String::from_utf8_lossy(&out2.stderr).into_owned();
        if !out2.status.success() {
            return Err(Error::Transcribe(format!(
                "sherpa subprocess failed (after CPU fallback): {}",
                stderr2.trim()
            )));
        }
        return Ok((stdout2, stderr2, start.elapsed().as_secs_f64()));
    }
    Err(Error::Transcribe(format!(
        "sherpa subprocess failed: {}",
        stderr.trim()
    )))
}

fn exec(bin: &Path, args: &[String], cancelled: &dyn Fn() -> bool) -> Result<std::process::Output> {
    if cancelled() {
        return Err(Error::Cancelled);
    }
    let mut cmd = build_command(bin);
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    loop {
        if cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Cancelled);
        }
        if child.try_wait()?.is_some() {
            return Ok(child.wait_with_output()?);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

fn uses_cuda(args: &[String]) -> bool {
    args.iter().any(|a| a == "--provider=cuda")
}

fn swap_provider_to_cpu(args: &[String]) -> Vec<String> {
    args.iter()
        .map(|a| {
            if a == "--provider=cuda" {
                "--provider=cpu".to_owned()
            } else {
                a.clone()
            }
        })
        .collect()
}

pub fn is_cuda_load_failure(stderr: &str) -> bool {
    let s = stderr.to_lowercase();
    s.contains("cudnn")
        || s.contains("cudaproviderfactory")
        || s.contains("onnxruntime_providers_cuda")
        || (s.contains("cuda") && s.contains("error loading"))
}

fn truncate_for_log(s: &str, max: usize) -> String {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return "<empty>".into();
    }
    let one_line: String = trimmed
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" | ");
    if one_line.chars().count() <= max {
        one_line
    } else {
        let truncated: String = one_line.chars().take(max).collect();
        format!("{truncated}… [+{} chars]", one_line.chars().count() - max)
    }
}

fn cuda_failure_reason(stderr: &str) -> String {
    for line in stderr.lines() {
        let l = line.trim();
        if l.is_empty() {
            continue;
        }
        let lower = l.to_lowercase();
        if lower.contains("cudnn") || lower.contains("error loading") {
            return l.chars().take(200).collect();
        }
    }
    "CUDA provider failed to initialize".into()
}

fn build_command(bin: &Path) -> Command {
    let mut cmd = quiet_command(bin.as_os_str());
    if let Some((env_name, value)) = crate::runtimes::cudnn::augmented_library_path() {
        cmd.env(env_name, value);
    }
    cmd
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

    #[test]
    fn binary_name_matches_target_os() {
        let n = binary_name();
        let is_exe = std::path::Path::new(n)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"));
        assert_eq!(is_exe, cfg!(windows));
    }

    #[test]
    fn detects_cuda_provider_arg() {
        let with_cuda = vec!["--provider=cuda".to_owned(), "input.wav".to_owned()];
        assert!(uses_cuda(&with_cuda));
        let cpu = vec!["--provider=cpu".to_owned()];
        assert!(!uses_cuda(&cpu));
    }

    #[test]
    fn swap_provider_replaces_only_cuda_arg() {
        let args = vec![
            "--threads=4".to_owned(),
            "--provider=cuda".to_owned(),
            "input.wav".to_owned(),
        ];
        let swapped = swap_provider_to_cpu(&args);
        assert_eq!(swapped[0], "--threads=4");
        assert_eq!(swapped[1], "--provider=cpu");
        assert_eq!(swapped[2], "input.wav");
    }

    #[test]
    fn detects_known_cuda_failure_signatures() {
        assert!(is_cuda_load_failure("Error: cudnn library not found"));
        assert!(is_cuda_load_failure("CUDAProviderFactory init failed"));
        assert!(is_cuda_load_failure(
            "error loading onnxruntime_providers_cuda.dll"
        ));
        assert!(is_cuda_load_failure("CUDA error loading driver"));
    }

    #[test]
    fn ignores_unrelated_failure_messages() {
        assert!(!is_cuda_load_failure("invalid input shape"));
        assert!(!is_cuda_load_failure("OOM allocating tensor"));
    }

    #[test]
    fn cuda_failure_reason_picks_first_relevant_line() {
        let stderr = "noise line\n   error loading cudnn_ops64_9.dll\nlater";
        let reason = cuda_failure_reason(stderr);
        assert!(reason.contains("cudnn_ops64_9.dll"));
    }

    #[test]
    fn cuda_failure_reason_falls_back_when_no_match() {
        assert_eq!(
            cuda_failure_reason("no relevant lines here"),
            "CUDA provider failed to initialize"
        );
    }
}
