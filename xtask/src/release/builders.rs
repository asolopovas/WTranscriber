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
    run_streamed(
        "host",
        "bun",
        &[
            "run",
            "tauri",
            "build",
            "-c",
            "{\"build\":{\"beforeBuildCommand\":\"\"}}",
            "--",
            "--no-default-features",
            "--features",
            "sherpa-static",
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
        println!("[wsl] --skip-rebuild, looking for existing .deb");
        return Ok(0);
    }
    let wsl_script = format!("{}/scripts/wsl-build-deb.sh", win_path_to_wsl(&root()));
    run_streamed("wsl", "wsl", &["--", "bash", &wsl_script], &[], lock)
}
