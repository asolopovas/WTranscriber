use std::fs;
use std::path::Path;

use anyhow::Result;

use super::artifacts::win_path_to_wsl;
use crate::util::{SharedOut, root, run_streamed, run_streamed_stdin};

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
    run_streamed(
        "host",
        "bun",
        &[
            "run",
            "tauri",
            "build",
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
    let mut env_vars: Vec<(&str, &str)> = vec![("CARGO_INCREMENTAL", "1")];
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
    let probe_ok = std::process::Command::new("wsl")
        .args([
            "--",
            "bash",
            "-lc",
            "command -v bun && command -v cargo && echo READY",
        ])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("READY"))
        .unwrap_or(false);
    if !probe_ok {
        println!("[wsl] skipping (no distro with bun + cargo)");
        return Ok(-1);
    }
    let wsl_root = win_path_to_wsl(&root());
    let script = format!(
        "set -e\n\
         cd \"{wsl_root}\"\n\
         export CARGO_TARGET_DIR=\"$HOME/.cache/wtranscriber-wsl-target\"\n\
         export CARGO_INCREMENTAL=1\n\
         unset SHERPA_ONNX_LIB_DIR SHERPA_ONNX_LIB SHERPA_ONNX_INCLUDE_DIR\n\
         mkdir -p \"$CARGO_TARGET_DIR\"\n\
         bun install --frozen-lockfile --no-progress 2>&1 | tail -5\n\
         bun run tauri build --bundles deb -- --no-default-features --features sherpa-static\n"
    );
    run_streamed_stdin("wsl", "wsl", &["--", "bash", "-l"], &script, &[], lock)
}
