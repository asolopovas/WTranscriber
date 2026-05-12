use std::fs;
use std::path::Path;

use anyhow::Result;

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
    if cfg!(target_os = "linux") {
        println!("[host] building .deb inside debian:12 container (linux host)");
        return build_deb_in_docker(lock);
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
    if std::env::var("WT_ANDROID_NATIVE").is_ok() {
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
        return run_streamed(
            "and",
            std::env::current_exe()?.to_string_lossy().as_ref(),
            &["android", "build", "--target", "aarch64"],
            &env_vars,
            lock,
        );
    }
    build_android_in_docker(dev, lock)
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

pub(super) fn build_deb_docker(skip: bool, lock: &SharedOut) -> Result<i32> {
    if skip {
        println!("[deb] --skip-rebuild, looking for existing .deb");
        return Ok(0);
    }
    build_deb_in_docker(lock)
}

fn builder_image() -> String {
    std::env::var("WT_BUILDER_IMAGE").unwrap_or_else(|_| "wt-builder:debian12".into())
}

fn builder_volumes() -> (String, String, String, String) {
    (
        std::env::var("WT_BUILDER_CARGO_VOL").unwrap_or_else(|_| "wt-builder-cargo".into()),
        std::env::var("WT_BUILDER_TARGET_VOL").unwrap_or_else(|_| "wt-builder-target".into()),
        std::env::var("WT_BUILDER_BUN_VOL").unwrap_or_else(|_| "wt-builder-bun".into()),
        std::env::var("WT_BUILDER_GRADLE_VOL").unwrap_or_else(|_| "wt-builder-gradle".into()),
    )
}

fn root_for_mount() -> String {
    let r = root().to_string_lossy().to_string();
    if cfg!(target_os = "windows") {
        r.replace('\\', "/")
    } else {
        r
    }
}

fn ensure_builder_image(image: &str, lock: &SharedOut) -> Result<()> {
    let rebuild = std::env::var("WT_BUILDER_REBUILD").ok().as_deref() == Some("1");
    let need_build = rebuild
        || std::process::Command::new("docker")
            .args(["image", "inspect", image])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| !s.success())
            .unwrap_or(true);
    if !need_build {
        return Ok(());
    }
    println!("[docker] building image {image} from Dockerfile.builder");
    let rc = run_streamed(
        "docker",
        "docker",
        &["build", "-f", "Dockerfile.builder", "-t", image, "."],
        &[],
        lock,
    )?;
    if rc != 0 {
        anyhow::bail!("docker build failed (exit {rc})");
    }
    Ok(())
}

fn ensure_volume(tag: &str, name: &str, lock: &SharedOut) -> Result<()> {
    let exists = std::process::Command::new("docker")
        .args(["volume", "inspect", name])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if exists {
        return Ok(());
    }
    let rc = run_streamed(tag, "docker", &["volume", "create", name], &[], lock)?;
    if rc != 0 {
        anyhow::bail!("docker volume create {name} failed (exit {rc})");
    }
    Ok(())
}

fn build_deb_in_docker(lock: &SharedOut) -> Result<i32> {
    let image = builder_image();
    let (vol_cargo, vol_target, vol_bun, _vol_gradle) = builder_volumes();

    ensure_builder_image(&image, lock)?;

    ensure_volume("deb", &vol_cargo, lock)?;
    ensure_volume("deb", &vol_target, lock)?;
    ensure_volume("deb", &vol_bun, lock)?;

    let inner = r#"
set -euo pipefail
unset SHERPA_ONNX_LIB_DIR SHERPA_ONNX_LIB SHERPA_ONNX_INCLUDE_DIR || true
bun install --frozen-lockfile --no-progress 2>&1 | tail -5
bun run tauri build --bundles deb -c '{"build":{"beforeBuildCommand":""}}' -- --no-default-features --features sherpa-static
SRC="/cache/target/release/bundle/deb"
DST="/work/src-tauri/target/release/bundle/deb"
mkdir -p "$DST"
cp -f "$SRC"/*.deb "$DST"/
"#;

    println!("[deb] running build inside {image}");
    let mount = format!("{}:/work", root_for_mount());
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

fn build_android_in_docker(dev: bool, lock: &SharedOut) -> Result<i32> {
    let image = builder_image();
    let (vol_cargo, vol_target, vol_bun, vol_gradle) = builder_volumes();

    ensure_builder_image(&image, lock)?;
    ensure_volume("and", &vol_cargo, lock)?;
    ensure_volume("and", &vol_target, lock)?;
    ensure_volume("and", &vol_bun, lock)?;
    ensure_volume("and", &vol_gradle, lock)?;

    // The container's NDK + SDK already live at /opt/android-sdk; xtask's
    // android/paths.rs reads NDK_HOME, which the image sets. sherpa-onnx
    // prebuilts still come from .android-prebuilt (downloaded by xtask on
    // first run, lives on the bind-mounted /work).
    let dev_env = if dev { "WT_DEV_APK=1" } else { "" };
    let inner = format!(
        r#"
set -euo pipefail
unset SHERPA_ONNX_LIB_DIR SHERPA_ONNX_LIB SHERPA_ONNX_INCLUDE_DIR || true
KS=/work/src-tauri/gen/android/debug.keystore
KP=/work/src-tauri/gen/android/keystore.properties
if [[ ! -f "$KS" ]]; then
  keytool -genkeypair -keystore "$KS" -storepass android -keypass android \
    -alias androiddebugkey -keyalg RSA -keysize 2048 -validity 10000 \
    -dname 'CN=Android Debug, O=Android, C=US' >/dev/null
fi
if [[ ! -f "$KP" ]]; then
  printf 'storeFile=%s\nstorePassword=android\nkeyAlias=androiddebugkey\nkeyPassword=android\n' "$KS" > "$KP"
fi
bun install --frozen-lockfile --no-progress 2>&1 | tail -5
{dev_env} WT_SKIP_FRONTEND=1 cargo run --manifest-path xtask/Cargo.toml --quiet -- android build --target aarch64
"#
    );

    println!("[and] running android build inside {image}");
    let mount = format!("{}:/work", root_for_mount());
    let vmc = format!("{vol_cargo}:/cache/cargo");
    let vmt = format!("{vol_target}:/cache/target");
    let vmb = format!("{vol_bun}:/cache/bun");
    let vmg = format!("{vol_gradle}:/root/.gradle");
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
        "-v",
        &vmg,
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
        &inner,
    ];
    run_streamed("and", "docker", &args, &[], lock)
}
