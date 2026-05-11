use std::path::Path;

use crate::{
    logfile,
    runtimes::{cudnn, sherpa},
};

const CUDA_DLL_NAMES: &[&str] = &[
    "onnxruntime.dll",
    "onnxruntime_providers_cuda.dll",
    "onnxruntime_providers_shared.dll",
    "onnxruntime_providers_tensorrt.dll",
];

pub fn setup() {
    if !cfg!(feature = "cuda") {
        logfile::info("inproc-cuda: skipped (cuda feature off)");
        return;
    }
    if cfg!(target_os = "linux") {
        setup_linux();
        return;
    }
    if !cfg!(target_os = "windows") {
        logfile::info("inproc-cuda: skipped (unsupported os)");
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

    prepend_cudnn_to_process_path();
}

fn setup_linux() {
    if let Some(bin) = cudnn::library_dir() {
        let dll = bin.join(cudnn::target_dll());
        if dll.exists() {
            prepend_to_env("LD_LIBRARY_PATH", &bin);
            logfile::info(&format!(
                "inproc-cuda: cuDNN dir on LD_LIBRARY_PATH: {}",
                bin.display()
            ));
            preload_cudnn_libs(&bin);
        } else {
            logfile::warn(&format!(
                "inproc-cuda: cuDNN missing at {}; CUDA EP will fail to load. \
                 Launch the app once to auto-install or run the equivalent install step.",
                dll.display()
            ));
        }
    } else {
        logfile::warn("inproc-cuda: cuDNN library_dir unresolved");
    }

    if let Ok(sherpa_lib) = sherpa::install_dir(sherpa::Variant::Cuda).map(|d| d.join("lib")) {
        if sherpa_lib.is_dir() {
            prepend_to_env("LD_LIBRARY_PATH", &sherpa_lib);
            logfile::info(&format!(
                "inproc-cuda: sherpa CUDA lib dir on LD_LIBRARY_PATH: {}",
                sherpa_lib.display()
            ));
        }
    }
}

#[cfg(target_os = "linux")]
fn preload_cudnn_libs(dir: &Path) {
    let candidates = [
        "libcudnn.so.9",
        "libcudnn_graph.so.9",
        "libcudnn_ops.so.9",
        "libcudnn_engines_precompiled.so.9",
        "libcudnn_engines_runtime_compiled.so.9",
        "libcudnn_heuristic.so.9",
        "libcudnn_adv.so.9",
        "libcudnn_cnn.so.9",
    ];
    let mut loaded = 0u32;
    let mut failed: Vec<String> = Vec::new();
    for name in candidates {
        let p = dir.join(name);
        if !p.exists() {
            continue;
        }
        match dlopen_global(&p) {
            Ok(()) => loaded += 1,
            Err(e) => failed.push(format!("{name}: {e}")),
        }
    }
    if loaded == 0 {
        logfile::warn(
            "inproc-cuda: failed to preload any cuDNN library; ONNX Runtime CUDA EP \
             will likely fall back to CPU.",
        );
    } else {
        logfile::info(&format!(
            "inproc-cuda: preloaded {loaded} cuDNN libraries via RTLD_GLOBAL"
        ));
    }
    if !failed.is_empty() {
        logfile::warn(&format!(
            "inproc-cuda: cuDNN preload partial failures: {}",
            failed.join("; ")
        ));
    }
}

#[cfg(target_os = "linux")]
fn dlopen_global(path: &Path) -> std::result::Result<(), String> {
    use std::ffi::CString;
    let c = CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|e| format!("path contains NUL: {e}"))?;

    let flags = libc::RTLD_NOW | libc::RTLD_GLOBAL;

    #[allow(unsafe_code)]
    let handle = unsafe { libc::dlopen(c.as_ptr(), flags) };
    if handle.is_null() {
        #[allow(unsafe_code)]
        let err = unsafe {
            let e = libc::dlerror();
            if e.is_null() {
                String::from("unknown dlopen error")
            } else {
                std::ffi::CStr::from_ptr(e).to_string_lossy().into_owned()
            }
        };
        return Err(err);
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn preload_cudnn_libs(_dir: &Path) {}

fn prepend_to_env(name: &str, dir: &Path) {
    let current = std::env::var_os(name).unwrap_or_default();
    let sep = if cfg!(windows) { ";" } else { ":" };
    let already = current.to_string_lossy().split(sep).any(|p| {
        let p = Path::new(p);
        p == dir
            || p.canonicalize()
                .ok()
                .zip(dir.canonicalize().ok())
                .is_some_and(|(a, b)| a == b)
    });
    if already {
        return;
    }
    let mut new_path = std::ffi::OsString::from(dir.as_os_str());
    if !current.is_empty() {
        new_path.push(sep);
        new_path.push(&current);
    }
    #[allow(unsafe_code)]
    unsafe {
        std::env::set_var(name, &new_path);
    }
}

fn prepend_cudnn_to_process_path() {
    let Some(bin) = cudnn::bin_dir() else {
        logfile::warn("inproc-cuda: cudnn bin_dir unresolved");
        return;
    };
    let dll = bin.join(cudnn::target_dll());
    if !dll.exists() {
        logfile::warn(&format!(
            "inproc-cuda: cuDNN missing at {}; CUDA EP will fail to load. \
             Run the app once with internet to auto-install, or run `just cudnn`.",
            dll.display()
        ));
        return;
    }
    let current = std::env::var_os("PATH").unwrap_or_default();
    let sep = if cfg!(windows) { ";" } else { ":" };
    let already = current.to_string_lossy().split(sep).any(|p| {
        let p = Path::new(p);
        p == bin
            || p.canonicalize()
                .ok()
                .zip(bin.canonicalize().ok())
                .is_some_and(|(a, b)| a == b)
    });
    if already {
        logfile::info(&format!(
            "inproc-cuda: cuDNN bin already on process PATH ({})",
            bin.display()
        ));
        return;
    }
    let mut new_path = std::ffi::OsString::from(bin.as_os_str());
    new_path.push(sep);
    new_path.push(&current);
    #[allow(unsafe_code)]
    unsafe {
        std::env::set_var("PATH", &new_path);
    }
    logfile::info(&format!(
        "inproc-cuda: prepended cuDNN bin to process PATH: {}",
        bin.display()
    ));
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
