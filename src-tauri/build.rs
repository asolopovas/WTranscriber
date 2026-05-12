use std::path::PathBuf;

fn main() {
    tauri_build::build();
    point_sherpa_lib_dir_to_cuda();
    install_cuda_dlls();
}

// When the `cuda` feature is on, redirect the sherpa-onnx crate's build
// script to link against the GPU-enabled shared libraries already downloaded
// by the app (sherpa-onnx-cuda runtime), instead of the crate's default
// auto-downloaded CPU-only `-shared-lib` archive. Without this, on Linux the
// in-process FFI silently runs CPU.
fn point_sherpa_lib_dir_to_cuda() {
    if std::env::var("CARGO_FEATURE_CUDA").is_err() {
        return;
    }
    // Respect explicit override.
    println!("cargo:rerun-if-env-changed=SHERPA_ONNX_LIB_DIR");
    if std::env::var_os("SHERPA_ONNX_LIB_DIR").is_some() {
        return;
    }
    let Some(lib_dir) = cuda_runtime_lib_dir() else {
        println!(
            "cargo:warning=in-process CUDA: sherpa-onnx GPU runtime not found; \
             launch wtranscriber once to auto-install it, then rebuild (or set \
             SHERPA_ONNX_LIB_DIR). The in-process recognizer will run on CPU \
             until then."
        );
        return;
    };
    println!("cargo:rerun-if-changed={}", lib_dir.display());
    println!("cargo:rustc-env=SHERPA_ONNX_LIB_DIR={}", lib_dir.display());
    // Belt-and-braces: also embed an absolute rpath so the binary loads the
    // GPU .so's at runtime even if the crate's auto-copy step is skipped.
    if cfg!(target_os = "linux") {
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
    }
}

fn install_cuda_dlls() {
    if std::env::var("CARGO_FEATURE_CUDA").is_err() {
        return;
    }
    if !cfg!(target_os = "windows") {
        return;
    }
    let Some(dst_dir) = target_profile_dir() else {
        println!("cargo:warning=cuda-dll-install: cannot resolve target profile dir");
        return;
    };
    let Some(src_dir) = cuda_runtime_bin_dir() else {
        println!(
            "cargo:warning=cuda-dll-install: cuda runtime not yet installed; \
             launch the app once so it downloads sherpa-onnx-cuda, then rebuild"
        );
        return;
    };

    println!("cargo:rerun-if-changed={}", src_dir.display());
    println!("cargo:rerun-if-env-changed=WT_CUDA_DLL_SRC");

    let names = [
        "onnxruntime.dll",
        "onnxruntime_providers_cuda.dll",
        "onnxruntime_providers_shared.dll",
        "onnxruntime_providers_tensorrt.dll",
    ];
    for name in names {
        let src = src_dir.join(name);
        let dst = dst_dir.join(name);
        if !src.exists() {
            println!("cargo:warning=cuda-dll-install: {name} missing in source");
            continue;
        }
        if same_file(&src, &dst) {
            continue;
        }
        match std::fs::copy(&src, &dst) {
            Ok(_) => {}
            Err(e) => {
                if same_file(&src, &dst) {
                    continue;
                }
                println!(
                    "cargo:warning=cuda-dll-install: copy {name} failed: {e} (src={}, dst={})",
                    src.display(),
                    dst.display()
                );
            }
        }
    }
}

fn same_file(a: &std::path::Path, b: &std::path::Path) -> bool {
    let (Ok(ma), Ok(mb)) = (std::fs::metadata(a), std::fs::metadata(b)) else {
        return false;
    };
    if ma.len() != mb.len() {
        return false;
    }
    match (ma.modified(), mb.modified()) {
        (Ok(ta), Ok(tb)) => ta == tb,
        _ => false,
    }
}

fn target_profile_dir() -> Option<PathBuf> {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR")?);
    let profile = std::env::var("PROFILE").ok()?;
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| manifest.join("target"), PathBuf::from);
    let dir = target.join(&profile);
    dir.is_dir().then_some(dir)
}

fn cuda_runtime_bin_dir() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("WT_CUDA_DLL_SRC") {
        let p = PathBuf::from(p);
        return p.is_dir().then_some(p);
    }
    cuda_runtime_root()
        .map(|root| root.join("bin"))
        .filter(|p| p.is_dir())
}

fn cuda_runtime_lib_dir() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("WT_SHERPA_CUDA_LIB_DIR") {
        let p = PathBuf::from(p);
        return p.is_dir().then_some(p);
    }
    cuda_runtime_root()
        .map(|root| root.join("lib"))
        .filter(|p| p.is_dir())
}

fn cuda_runtime_root() -> Option<PathBuf> {
    let version = include_str!("sherpa-version.txt").trim_end();
    #[allow(dead_code, clippy::items_after_statements)]
    mod ident {
        include!("../shared/identity.rs");
    }
    let base = if cfg!(target_os = "windows") {
        let appdata = std::env::var_os("APPDATA")?;
        PathBuf::from(appdata)
            .join(ident::APP_ORG)
            .join(ident::APP_NAME)
            .join("data")
    } else if cfg!(target_os = "linux") {
        let home = std::env::var_os("HOME")?;
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join(ident::APP_NAME)
    } else if cfg!(target_os = "macos") {
        let home = std::env::var_os("HOME")?;
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join(ident::APP_ID)
    } else {
        return None;
    };
    Some(
        base.join("third_party")
            .join("sherpa-onnx")
            .join(format!("{version}-cuda")),
    )
}
