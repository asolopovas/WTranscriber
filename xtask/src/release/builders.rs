use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::util::{SharedOut, root, run_streamed};

const WINDOWS_CUDA_ARCHITECTURES: &str = "61;75;80;86;89";

fn cargo_incremental_env() -> Vec<(&'static str, &'static str)> {
    // sccache wraps cmake-launched cl.exe (CMAKE_{C,CXX}_COMPILER_LAUNCHER is
    // baked into whisper-rs-sys's CMake build files at first configure) and
    // refuses to run if CARGO_INCREMENTAL=1. Release builds don't meaningfully
    // benefit from cargo's incremental compilation, so always disable it for
    // host release builds and clear any inherited value before spawn.
    // SAFETY: builders.rs is single-threaded at entry; spawned build threads
    // have not yet captured env, so removal is observed by their children.
    unsafe { std::env::remove_var("CARGO_INCREMENTAL") };
    Vec::new()
}

fn whisper_release_build_dirs() -> Vec<PathBuf> {
    let build_dir = root()
        .join("src-tauri")
        .join("target")
        .join("release")
        .join("build");
    let Ok(entries) = fs::read_dir(build_dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("whisper-rs-sys-"))
        })
        .collect()
}

fn whisper_release_cuda_arch_stale() -> bool {
    let expected = format!("CMAKE_CUDA_ARCHITECTURES:UNINITIALIZED={WINDOWS_CUDA_ARCHITECTURES}");
    for dir in whisper_release_build_dirs() {
        let cache = dir.join("out").join("build").join("CMakeCache.txt");
        let Ok(raw) = fs::read_to_string(cache) else {
            continue;
        };
        if raw.lines().any(|line| line.trim() == expected) {
            return false;
        }
        if raw
            .lines()
            .any(|line| line.starts_with("CMAKE_CUDA_ARCHITECTURES:"))
        {
            return true;
        }
    }
    false
}

fn clean_whisper_release_build_dirs() -> Result<()> {
    for path in whisper_release_build_dirs() {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

pub(super) fn build_host(skip: bool, lock: &SharedOut) -> Result<i32> {
    if skip {
        println!("[host] --skip-rebuild, reusing existing bundle");
        return Ok(0);
    }
    if whisper_release_cuda_arch_stale() {
        clean_whisper_release_build_dirs()?;
        let clean_rc = run_streamed(
            "host-clean",
            "cargo",
            &[
                "clean",
                "--manifest-path",
                "src-tauri/Cargo.toml",
                "-p",
                "whisper-rs-sys",
            ],
            &[],
            lock,
        )?;
        if clean_rc != 0 {
            return Ok(clean_rc);
        }
    }
    let incr_gui = cargo_incremental_env();
    let rc_gui = run_streamed(
        "host-gui",
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
            "directml",
        ],
        &incr_gui,
        lock,
    )?;
    Ok(rc_gui)
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
        let mut env_vars: Vec<(&str, &str)> = cargo_incremental_env();
        env_vars.push(("WT_SKIP_FRONTEND", "1"));
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

pub fn ensure_dev_keystore_properties(dev: bool) -> Result<()> {
    let ks_props = root()
        .join("src-tauri")
        .join("gen")
        .join("android")
        .join("keystore.properties");
    let needs_write = if ks_props.exists() {
        let recorded = fs::read_to_string(&ks_props)
            .unwrap_or_default()
            .lines()
            .find_map(|l| l.strip_prefix("storeFile=").map(str::to_string))
            .unwrap_or_default();
        !recorded.is_empty() && !Path::new(&recorded).exists()
    } else {
        true
    };
    if !needs_write {
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
    eprintln!("[and] (re)wrote {} → {}", ks_props.display(), store_path);
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

fn docker_parallel_env_args() -> Vec<String> {
    let mut out = Vec::new();
    for (key, value) in crate::util::parallel_build_env(crate::util::parallel_jobs()) {
        out.push("-e".into());
        out.push(format!("{key}={value}"));
    }
    out
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
flock /cache/bun/install.lock bun install --frozen-lockfile --no-progress 2>&1 | tail -5
SRC="/cache/target/release/bundle/deb"
mkdir -p "$SRC"
rm -f "$SRC"/*.deb
bun run tauri build --bundles deb -c '{"build":{"beforeBuildCommand":""}}' -- --no-default-features --features sherpa-static
for deb in "$SRC"/*.deb; do
  pkg="$(mktemp -d)"
  out="$deb.repacked"
  dpkg-deb -R "$deb" "$pkg"
  if [[ -d "$pkg/usr/bin" ]]; then
    find "$pkg/usr/bin" -maxdepth 1 -type f -perm -0100 -exec strip --strip-unneeded {} +
  fi
  dpkg-deb -Zxz -z9 -b "$pkg" "$out" >/dev/null
  mv -f "$out" "$deb"
  rm -rf "$pkg"
  du -h "$deb"
done
DST="/work/src-tauri/target/release/bundle/deb"
mkdir -p "$DST"
cp -f "$SRC"/*.deb "$DST"/
"#;

    println!("[deb] running build inside {image}");
    let mount = format!("{}:/work", root_for_mount());
    let vmc = format!("{vol_cargo}:/cache/cargo");
    let vmt = format!("{vol_target}:/cache/target");
    let vmb = format!("{vol_bun}:/cache/bun");
    let mut args = vec![
        "run".into(),
        "--rm".into(),
        "-v".into(),
        mount,
        "-v".into(),
        vmc,
        "-v".into(),
        vmt,
        "-v".into(),
        vmb,
        "-e".into(),
        "CARGO_TARGET_DIR=/cache/target".into(),
        "-e".into(),
        "CARGO_INCREMENTAL=1".into(),
        "-e".into(),
        "BUN_INSTALL_CACHE_DIR=/cache/bun".into(),
    ];
    args.extend(docker_parallel_env_args());
    args.extend([
        "-w".into(),
        "/work".into(),
        image,
        "bash".into(),
        "-lc".into(),
        inner.into(),
    ]);
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    run_streamed("deb", "docker", &arg_refs, &[], lock)
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
RECORDED_KS=""
if [[ -f "$KP" ]]; then
  RECORDED_KS="$(sed -n 's/^storeFile=//p' "$KP" | head -1)"
fi
if [[ ! -f "$KP" || ! -f "$RECORDED_KS" ]]; then
  printf 'storeFile=%s\nstorePassword=android\nkeyAlias=androiddebugkey\nkeyPassword=android\n' "$KS" > "$KP"
fi
flock /cache/bun/install.lock bun install --frozen-lockfile --no-progress 2>&1 | tail -5
{dev_env} WT_SKIP_FRONTEND=1 cargo run --manifest-path xtask/Cargo.toml --quiet -- android build --target aarch64
"#
    );

    println!("[and] running android build inside {image}");
    let mount = format!("{}:/work", root_for_mount());
    let vmc = format!("{vol_cargo}:/cache/cargo");
    let vmt = format!("{vol_target}:/cache/target");
    let vmb = format!("{vol_bun}:/cache/bun");
    let vmg = format!("{vol_gradle}:/root/.gradle");
    let mut args = vec![
        "run".into(),
        "--rm".into(),
        "-v".into(),
        mount,
        "-v".into(),
        vmc,
        "-v".into(),
        vmt,
        "-v".into(),
        vmb,
        "-v".into(),
        vmg,
        "-e".into(),
        "CARGO_TARGET_DIR=/cache/target".into(),
        "-e".into(),
        "CARGO_INCREMENTAL=1".into(),
        "-e".into(),
        "BUN_INSTALL_CACHE_DIR=/cache/bun".into(),
    ];
    args.extend(docker_parallel_env_args());
    args.extend([
        "-w".into(),
        "/work".into(),
        image,
        "bash".into(),
        "-lc".into(),
        inner,
    ]);
    let arg_refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    run_streamed("and", "docker", &arg_refs, &[], lock)
}
