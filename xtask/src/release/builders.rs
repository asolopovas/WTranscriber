use std::fs;
use std::path::Path;

use anyhow::Result;

use super::artifacts::win_path_to_wsl;
use crate::util::{SharedOut, root, run_streamed};

pub(super) fn build_host(skip: bool, lock: &SharedOut) -> Result<i32> {
    if skip {
        println!("[host] --skip-rebuild, reusing existing bundle");
        return Ok(0);
    }
    unsafe {
        std::env::remove_var("SHERPA_ONNX_LIB_DIR");
        std::env::remove_var("SHERPA_ONNX_LIB");
        std::env::remove_var("SHERPA_ONNX_INCLUDE_DIR");
    }
    if cfg!(target_os = "linux") && std::env::var("WT_HOST_DEB_DOCKER").is_ok() {
        println!("[host] building .deb inside debian:12 container (WT_HOST_DEB_DOCKER set)");
        return run_streamed("host", "bash", &["docker/build-deb.sh"], &[], lock);
    }
    let rc = run_streamed(
        "host",
        "cargo",
        &[
            "build",
            "--manifest-path",
            "src-tauri/Cargo.toml",
            "--release",
            "--bin",
            "wt",
        ],
        &[("CARGO_INCREMENTAL", "1")],
        lock,
    )?;
    if rc != 0 {
        return Ok(rc);
    }
    run_streamed(
        "host",
        "bun",
        &[
            "run",
            "tauri",
            "build",
            "-c",
            "{\"build\":{\"beforeBuildCommand\":\"\"}}",
        ],
        &[("CARGO_INCREMENTAL", "1")],
        lock,
    )
}

pub(super) fn build_android(skip: bool, dev: bool, lock: &SharedOut) -> Result<i32> {
    if skip {
        println!("[and] --skip-rebuild, reusing existing apk");
        return Ok(0);
    }
    let rc = crate::android::sign_patch_inline()?;
    if rc != 0 {
        return Ok(rc);
    }
    ensure_dev_keystore_properties(dev)?;
    let mut env_vars: Vec<(&str, &str)> =
        vec![("CARGO_INCREMENTAL", "1"), ("WT_SKIP_FRONTEND", "1")];
    if dev {
        env_vars.push(("WT_DEV_APK", "1"));
    }
    run_streamed(
        "and",
        std::env::current_exe()?.to_string_lossy().as_ref(),
        &["android", "build", "--target", "aarch64"],
        &env_vars,
        lock,
    )
}

pub(super) fn ensure_dev_keystore_properties(dev: bool) -> Result<()> {
    let ks_props = root()
        .join("src-tauri")
        .join("gen")
        .join("android")
        .join("keystore.properties");
    if ks_props.exists() {
        return Ok(());
    }
    if !dev {
        return Ok(());
    }
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    let debug_ks = Path::new(&home).join(".android").join("debug.keystore");
    if !debug_ks.exists() {
        eprintln!(
            "[and] no debug.keystore at {} — dev APK will be unsigned",
            debug_ks.display()
        );
        return Ok(());
    }
    if let Some(parent) = ks_props.parent() {
        fs::create_dir_all(parent)?;
    }
    let store_path = debug_ks.to_string_lossy().replace('\\', "/");
    let body = format!(
        "storeFile={store_path}\nstorePassword=android\nkeyAlias=androiddebugkey\nkeyPassword=android\n"
    );
    fs::write(&ks_props, body)?;
    eprintln!(
        "[and] generated {} pointing at debug.keystore",
        ks_props.display()
    );
    Ok(())
}

pub(super) fn build_wsl(skip: bool, lock: &SharedOut) -> Result<i32> {
    if skip {
        println!("[deb] --skip-rebuild, looking for existing .deb");
        return Ok(0);
    }
    if std::env::var("WT_DEB_VIA_WSL").is_ok() {
        let wsl_script = format!("{}/scripts/wsl-build-deb.sh", win_path_to_wsl(&root()));
        return run_streamed("wsl", "wsl", &["--", "bash", &wsl_script], &[], lock);
    }
    build_deb_in_docker(lock)
}

fn build_deb_in_docker(lock: &SharedOut) -> Result<i32> {
    let image = std::env::var("WT_DEB_IMAGE").unwrap_or_else(|_| "wt-deb-builder:debian12".into());
    let vol_cargo = std::env::var("WT_DEB_CARGO_VOL").unwrap_or_else(|_| "wt-deb-cargo".into());
    let vol_target = std::env::var("WT_DEB_TARGET_VOL").unwrap_or_else(|_| "wt-deb-target".into());
    let vol_bun = std::env::var("WT_DEB_BUN_VOL").unwrap_or_else(|_| "wt-deb-bun".into());

    let root_path = root();
    let root_str = root_path.to_string_lossy().to_string();
    let rebuild = std::env::var("WT_DEB_REBUILD").ok().as_deref() == Some("1");

    let need_build = rebuild
        || std::process::Command::new("docker")
            .args(["image", "inspect", &image])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| !s.success())
            .unwrap_or(true);
    if need_build {
        println!("[deb] building image {image}");
        let rc = run_streamed(
            "deb",
            "docker",
            &[
                "build",
                "-f",
                "docker/Dockerfile.deb",
                "-t",
                &image,
                "docker/",
            ],
            &[],
            lock,
        )?;
        if rc != 0 {
            return Ok(rc);
        }
    }

    for vol in [&vol_cargo, &vol_target, &vol_bun] {
        let exists = std::process::Command::new("docker")
            .args(["volume", "inspect", vol])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !exists {
            let rc = run_streamed("deb", "docker", &["volume", "create", vol], &[], lock)?;
            if rc != 0 {
                return Ok(rc);
            }
        }
    }

    let inner = r#"
set -euo pipefail
unset SHERPA_ONNX_LIB_DIR SHERPA_ONNX_LIB SHERPA_ONNX_INCLUDE_DIR || true
bun install --frozen-lockfile --no-progress 2>&1 | tail -5
bun run tauri build --bundles deb -- --no-default-features --features sherpa-static
SRC="/cache/target/release/bundle/deb"
DST="/work/src-tauri/target/release/bundle/deb"
mkdir -p "$DST"
cp -f "$SRC"/*.deb "$DST"/
"#;

    println!("[deb] running build inside {image}");
    let mount_src = if cfg!(target_os = "windows") {
        // Docker Desktop on Windows requires forward slashes
        root_str.replace('\\', "/")
    } else {
        root_str.clone()
    };
    let mount = format!("{mount_src}:/work");
    let vmc = format!("{vol_cargo}:/cache/cargo");
    let vmt = format!("{vol_target}:/cache/target");
    let vmb = format!("{vol_bun}:/cache/bun");
    let args = vec![
        "run",
        "--rm",
        "-v",
        &mount,
        "-v",
        &vmc,
        "-v",
        &vmt,
        "-v",
        &vmb,
        "-e",
        "CARGO_TARGET_DIR=/cache/target",
        "-e",
        "CARGO_INCREMENTAL=1",
        "-e",
        "BUN_INSTALL_CACHE_DIR=/cache/bun",
        "-w",
        "/work",
        &image,
        "bash",
        "-lc",
        inner,
    ];
    run_streamed("deb", "docker", &args, &[], lock)
}
