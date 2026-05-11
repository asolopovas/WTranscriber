use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::{config, logfile, models, paths, runtimes};

#[derive(Debug, Clone, Serialize)]
struct RuntimeProgress {
    id: String,
    #[serde(flatten)]
    progress: models::download::ByteProgress,
}

pub async fn ensure_runtimes(app: &tauri::AppHandle) {
    if cfg!(target_os = "android") {
        logfile::info("runtime install skipped (android: jniLibs bundled in APK)");
        return;
    }
    let cfg = config::Config::load().unwrap_or_default();
    let gpu_present = has_nvidia_gpu();
    let effective_device = effective_device(cfg.device, gpu_present);
    let variant = runtimes::SherpaVariant::from_device(effective_device);

    let sherpa_static_cpu =
        cfg!(feature = "sherpa-static") && matches!(variant, runtimes::SherpaVariant::Cpu);
    if sherpa_static_cpu {
        logfile::info("runtime sherpa-onnx-cpu skipped (statically linked into binary)");
    } else {
        install_sherpa(app, variant).await;
    }

    if matches!(variant, runtimes::SherpaVariant::Cuda) && runtimes::cudnn_supported() {
        install_cudnn(app).await;
    }
    install_llama(app).await;
    if matches!(variant, runtimes::SherpaVariant::Cuda) {
        runtimes::inproc_cuda::setup();
        runtimes::inproc_cuda::dump_path();
    }

    let want_nemo_python = cfg.diarize && matches!(cfg.diarizer, config::DiarizerChoice::Nemo);
    if !cfg!(target_os = "windows") && want_nemo_python && gpu_present {
        spawn_nemo_runtime_install(app.clone());
    } else if want_nemo_python && !gpu_present {
        logfile::info(
            "runtime nemo-python skipped (no NVIDIA GPU detected; install on demand from settings)",
        );
    }
}

fn effective_device(requested: config::Device, gpu_present: bool) -> config::Device {
    if matches!(requested, config::Device::Cuda) && !gpu_present {
        logfile::info(
            "config device=cuda but no NVIDIA GPU detected; treating as cpu for runtime install",
        );
        return config::Device::Cpu;
    }
    requested
}

fn has_nvidia_gpu() -> bool {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        #[cfg(target_os = "linux")]
        if std::path::Path::new("/dev/nvidia0").exists() {
            return true;
        }
        match std::process::Command::new("nvidia-smi")
            .arg("-L")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
        {
            Ok(out) if out.status.success() => {
                let s = String::from_utf8_lossy(&out.stdout);
                s.lines().any(|l| l.starts_with("GPU "))
            }
            _ => false,
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        false
    }
}

static NEMO_RUNTIME_STARTED: AtomicBool = AtomicBool::new(false);

fn nemo_python_path() -> Option<PathBuf> {
    let data = paths::data_dir().ok()?;
    Some(data.join("python").join("bin").join("python3.12"))
}

const fn install_script_basename() -> &'static str {
    if cfg!(target_os = "windows") {
        "install-nemo-deps.ps1"
    } else {
        "install-nemo-deps.sh"
    }
}

fn locate_install_script() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("WT_NEMO_INSTALL_SCRIPT") {
        let path = PathBuf::from(p);
        if path.exists() {
            return Some(path);
        }
    }
    let basename = install_script_basename();
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join("_up_").join("scripts").join(basename));
        candidates.push(
            dir.join("..")
                .join("lib")
                .join("WTranscriber")
                .join("_up_")
                .join("scripts")
                .join(basename),
        );
        candidates.push(dir.join(basename));
        candidates.push(dir.join("resources").join(basename));
        candidates.push(dir.join("scripts").join(basename));
        candidates.push(
            dir.join("..")
                .join("Resources")
                .join("_up_")
                .join("scripts")
                .join(basename),
        );
        candidates.push(
            dir.join("..")
                .join("Resources")
                .join("scripts")
                .join(basename),
        );
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("scripts").join(basename));
        candidates.push(cwd.join("..").join("scripts").join(basename));
    }
    candidates.into_iter().find(|p| p.exists())
}

#[allow(clippy::too_many_lines)]
fn spawn_nemo_runtime_install(app: tauri::AppHandle) {
    if NEMO_RUNTIME_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    if let Some(py) = nemo_python_path()
        && py.exists()
    {
        let already_ready = std::process::Command::new(&py)
            .arg("-c")
            .arg("import nemo.collections.asr")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if already_ready {
            logfile::info("runtime nemo-python already installed");
            let _ = app.emit("runtime:done", "nemo-python");
            return;
        }
    }
    let Some(script) = locate_install_script() else {
        logfile::warn(&format!(
            "runtime nemo-python: {} not found",
            install_script_basename()
        ));
        return;
    };
    tauri::async_runtime::spawn(async move {
        let id = "nemo-python";
        logfile::info(&format!(
            "runtime install {id} starting (background; downloads ~5 GB on first run)"
        ));
        let _ = app.emit(
            "runtime:progress",
            &serde_json::json!({
                "id": id,
                "phase": "starting",
            }),
        );
        let mut cmd = if cfg!(target_os = "windows") {
            let mut c = Command::new("powershell");
            c.args([
                "-NoLogo",
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ]);
            c.arg(&script);
            c
        } else {
            let mut c = Command::new("bash");
            c.arg(&script);
            c
        };
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.kill_on_drop(false);
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                logfile::error(&format!("runtime install {id}: spawn: {e}"));
                let _ = app.emit("runtime:error", id);
                return;
            }
        };
        if let Some(stdout) = child.stdout.take() {
            let app = app.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    logfile::info(&format!("nemo-python: {line}"));
                    let _ = app.emit(
                        "runtime:progress",
                        &serde_json::json!({ "id": "nemo-python", "line": line }),
                    );
                }
            });
        }
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    logfile::warn(&format!("nemo-python: {line}"));
                }
            });
        }
        match child.wait().await {
            Ok(status) if status.success() => {
                logfile::info(&format!("runtime install {id} ok"));
                let _ = app.emit("runtime:done", id);
            }
            Ok(status) => {
                logfile::error(&format!("runtime install {id}: exit {status}"));
                let _ = app.emit("runtime:error", id);
            }
            Err(e) => {
                logfile::error(&format!("runtime install {id}: wait: {e}"));
                let _ = app.emit("runtime:error", id);
            }
        }
    });
}

async fn install_cudnn(app: &tauri::AppHandle) {
    let id = "cudnn".to_string();
    if runtimes::cudnn_installed() {
        logfile::info(&format!("runtime {id} already installed"));
    } else {
        logfile::info(&format!(
            "runtime install {id} starting (~700 MB, one-time)"
        ));
        let mut on_progress = progress_emitter(app, id.clone());
        match runtimes::ensure_cudnn(&mut on_progress).await {
            Ok(dll) => {
                logfile::info(&format!("runtime install {id} ok ({})", dll.display()));
                let _ = app.emit("runtime:done", &id);
            }
            Err(e) => {
                logfile::error(&format!("runtime install {id}: {e}"));
                let _ = app.emit("runtime:error", &id);
                return;
            }
        }
    }
    runtimes::cudnn::ensure_on_path();
}

async fn install_sherpa(app: &tauri::AppHandle, variant: runtimes::SherpaVariant) {
    let id = format!("sherpa-onnx-{}", variant.slug());
    if runtimes::sherpa_installed(variant) {
        logfile::info(&format!("runtime {id} already installed"));
        return;
    }
    logfile::info(&format!("runtime install {id} starting"));
    let mut on_progress = progress_emitter(app, id.clone());
    match runtimes::ensure_sherpa(variant, &mut on_progress).await {
        Ok(dir) => {
            logfile::info(&format!("runtime install {id} ok ({})", dir.display()));
            let _ = app.emit("runtime:done", &id);
        }
        Err(e) => {
            logfile::error(&format!("runtime install {id}: {e}"));
            let _ = app.emit("runtime:error", &id);
        }
    }
}

async fn install_llama(app: &tauri::AppHandle) {
    let id = "llama.cpp".to_string();
    if runtimes::llama_installed() {
        logfile::info(&format!("runtime {id} already installed"));
        return;
    }
    logfile::info(&format!("runtime install {id} starting"));
    let mut on_progress = progress_emitter(app, id.clone());
    match runtimes::ensure_llama(&mut on_progress).await {
        Ok(dir) => {
            logfile::info(&format!("runtime install {id} ok ({})", dir.display()));
            let _ = app.emit("runtime:done", &id);
        }
        Err(e) => {
            logfile::error(&format!("runtime install {id}: {e}"));
            let _ = app.emit("runtime:error", &id);
        }
    }
}

fn progress_emitter(
    app: &tauri::AppHandle,
    id: String,
) -> impl FnMut(models::download::Progress) + Send + use<> {
    let app = app.clone();
    move |p: models::download::Progress| {
        let _ = app.emit(
            "runtime:progress",
            &RuntimeProgress {
                id: id.clone(),
                progress: p,
            },
        );
    }
}
