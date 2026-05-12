#[cfg(any(target_os = "linux", target_os = "windows"))]
use std::process::Stdio;

use serde::Serialize;
use tauri::Emitter;

use crate::{config, logfile, models, runtimes};

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
