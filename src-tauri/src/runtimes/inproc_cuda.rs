use std::path::Path;

use crate::{logfile, runtimes::sherpa};

const CUDA_DLL_NAMES: &[&str] = &[
    "onnxruntime.dll",
    "onnxruntime_providers_cuda.dll",
    "onnxruntime_providers_shared.dll",
    "onnxruntime_providers_tensorrt.dll",
];

pub fn setup() {
    if !cfg!(feature = "cuda") || !cfg!(target_os = "windows") {
        logfile::info("inproc-cuda: skipped (feature/cuda or windows-only)");
        return;
    }

    let Ok(exe) = std::env::current_exe() else {
        logfile::warn("inproc-cuda: current_exe() failed");
        return;
    };
    let Some(exe_dir) = exe.parent() else {
        logfile::warn("inproc-cuda: exe has no parent directory");
        return;
    };

    logfile::info(&format!("inproc-cuda: exe dir = {}", exe_dir.display()));
    log_present_dlls(exe_dir);

    let Ok(cuda_bin) = sherpa::bin_dir(sherpa::Variant::Cuda) else {
        logfile::warn("inproc-cuda: sherpa cuda bin dir not resolvable");
        return;
    };
    if !cuda_bin.exists() {
        logfile::warn(&format!(
            "inproc-cuda: cuda runtime missing at {}",
            cuda_bin.display()
        ));
        return;
    }
    logfile::info(&format!(
        "inproc-cuda: cuda runtime source = {}",
        cuda_bin.display()
    ));

    let mut copied = 0u32;
    let mut skipped = 0u32;
    let mut missing_src: Vec<&str> = Vec::new();

    for name in CUDA_DLL_NAMES {
        let src = cuda_bin.join(name);
        let dst = exe_dir.join(name);
        if !src.exists() {
            missing_src.push(name);
            continue;
        }
        if dlls_equal(&src, &dst) {
            skipped += 1;
            continue;
        }
        match std::fs::copy(&src, &dst) {
            Ok(_) => {
                copied += 1;
                logfile::info(&format!("inproc-cuda: installed {name}"));
            }
            Err(e) => logfile::warn(&format!("inproc-cuda: copy {name} failed: {e}")),
        }
    }

    if !missing_src.is_empty() {
        logfile::warn(&format!(
            "inproc-cuda: missing in cuda runtime: {}",
            missing_src.join(", ")
        ));
    }

    let cuda_ep = exe_dir.join("onnxruntime_providers_cuda.dll");
    if cuda_ep.exists() {
        logfile::info(&format!(
            "inproc-cuda: ready (copied={copied}, up-to-date={skipped}); CUDA EP DLL present"
        ));
    } else {
        logfile::warn(
            "inproc-cuda: onnxruntime_providers_cuda.dll NOT present next to exe; \
             onnxruntime will silently fall back to CPU. Run `just sherpa-cuda` and \
             verify the cuda runtime archive includes the CUDA EP DLL.",
        );
    }
}

fn log_present_dlls(dir: &Path) {
    let mut found: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            let raw = entry.file_name();
            let is_dll = Path::new(&raw)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("dll"));
            if !is_dll {
                continue;
            }
            let name = raw.to_string_lossy().to_lowercase();
            if name.starts_with("onnxruntime") || name.starts_with("sherpa-onnx") {
                found.push(name);
            }
        }
    }
    found.sort();
    if found.is_empty() {
        logfile::info("inproc-cuda: exe-dir DLLs (onnxruntime/sherpa): <none>");
    } else {
        logfile::info(&format!(
            "inproc-cuda: exe-dir DLLs (onnxruntime/sherpa): {}",
            found.join(", ")
        ));
    }
}

fn dlls_equal(a: &Path, b: &Path) -> bool {
    let Ok(am) = std::fs::metadata(a) else {
        return false;
    };
    let Ok(bm) = std::fs::metadata(b) else {
        return false;
    };
    if am.len() != bm.len() {
        return false;
    }
    match (am.modified(), bm.modified()) {
        (Ok(ax), Ok(bx)) => ax == bx,
        _ => false,
    }
}

pub fn dump_path() {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let s = path.to_string_lossy();
    let entries: Vec<&str> = s.split(if cfg!(windows) { ';' } else { ':' }).collect();
    logfile::info(&format!("PATH entries: {}", entries.len()));
    for (i, e) in entries.iter().take(8).enumerate() {
        logfile::info(&format!("  PATH[{i}] = {e}"));
    }
    if entries.len() > 8 {
        logfile::info(&format!("  …({} more entries)", entries.len() - 8));
    }
}
