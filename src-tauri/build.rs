use std::path::PathBuf;

fn main() {
    tauri_build::build();
    install_cuda_dlls();
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
    let version = include_str!("sherpa-version.txt").trim_end();
    let appdata = std::env::var_os("APPDATA")?;
    let dir = PathBuf::from(appdata)
        .join("asolopovas")
        .join("wtranscriber")
        .join("data")
        .join("third_party")
        .join("sherpa-onnx")
        .join(format!("{version}-cuda"))
        .join("bin");
    dir.is_dir().then_some(dir)
}
