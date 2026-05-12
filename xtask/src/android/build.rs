use anyhow::{Context, Result, bail};
use std::fs;
use std::process::Command;
use std::time::Instant;

use crate::util::{exe, root, sh, sh_in};

use super::ANDROID_PACKAGE;
use super::patch::{
    copy_llama_jni, patch_gradle_build_config, patch_gradle_properties, patch_manifest,
    sign_patch_inline, sign_with_debug_keystore,
};
use super::paths::{
    abi_for, android_home, apk_release_dir, clang_ext, ndk_bin, ndk_home, prebuilt_dir,
};
use super::proc::spawn_with_env;

pub(super) fn build_env(target: &str) -> Result<Vec<(String, String)>> {
    let abi = abi_for(target)?;
    let sdk = android_home();
    let ndk = ndk_home(&sdk);
    let bin = ndk_bin(&ndk);
    let pdir = prebuilt_dir(target)?;
    let cc = bin.join(format!("{}{}", abi.clang, clang_ext()));
    let cxx = bin.join(format!("{}++{}", abi.clang, clang_ext()));
    let ar = bin.join(exe("llvm-ar"));
    let sysroot = bin
        .parent()
        .map(|p| p.join("sysroot"))
        .unwrap_or_else(|| ndk.join("sysroot"));
    let clang_target = abi.clang.trim_end_matches("-clang");
    let bindgen_args = format!("--target={} --sysroot={}", clang_target, sysroot.display());
    let mut e: Vec<(String, String)> = vec![
        ("ANDROID_HOME".into(), sdk.display().to_string()),
        ("NDK_HOME".into(), ndk.display().to_string()),
        ("SHERPA_ONNX_LIB_DIR".into(), pdir.display().to_string()),
        (format!("CC_{}", abi.rust), cc.display().to_string()),
        (format!("CXX_{}", abi.rust), cxx.display().to_string()),
        (format!("AR_{}", abi.rust), ar.display().to_string()),
        (
            format!("CARGO_TARGET_{}_LINKER", abi.rust.to_uppercase()),
            cc.display().to_string(),
        ),
        (
            format!("BINDGEN_EXTRA_CLANG_ARGS_{}", abi.rust),
            bindgen_args,
        ),
    ];
    e.sort();
    Ok(e)
}

pub(super) fn ensure_prebuilts(target: &str) -> Result<()> {
    let pdir = prebuilt_dir(target)?;
    if pdir.join("libsherpa-onnx-c-api.so").exists() {
        return Ok(());
    }
    eprintln!("sherpa-onnx prebuilts missing — fetching");
    cmd_prebuilts(None)
}

pub(super) fn prepare(target: &str, with_sign: bool) -> Result<Vec<(String, String)>> {
    ensure_prebuilts(target)?;
    if with_sign {
        sign_patch_inline()?;
    }
    patch_gradle_build_config()?;
    patch_gradle_properties()?;
    copy_llama_jni(target)?;
    patch_manifest()?;
    build_env(target)
}

pub(super) fn preflight_node_modules() -> Result<()> {
    let cli = root()
        .join("node_modules")
        .join("@tauri-apps")
        .join("cli")
        .join("package.json");
    if !cli.exists() {
        bail!(
            "node_modules missing (looked for {}). Run `bun install` before bootstrap.",
            cli.display()
        );
    }
    Ok(())
}

pub(super) fn cmd_doctor(target: &str) -> Result<()> {
    let abi = abi_for(target)?;
    let sdk = android_home();
    let ndk = ndk_home(&sdk);
    let bin = ndk_bin(&ndk);
    let sherpa = prebuilt_dir(target)?.join("libsherpa-onnx-c-api.so");
    for (k, v) in [
        ("OS", std::env::consts::OS.to_string()),
        ("ANDROID_HOME", sdk.display().to_string()),
        ("NDK_HOME", ndk.display().to_string()),
        ("NDK exists", ndk.exists().to_string()),
        ("NDK toolchain", bin.display().to_string()),
        ("target", target.to_string()),
        ("abi", abi.abi.to_string()),
        ("sherpa prebuilt", sherpa.display().to_string()),
        ("sherpa exists", sherpa.exists().to_string()),
    ] {
        println!("{k:<18} {v}");
    }
    Ok(())
}

pub(super) fn cmd_build(target: &str) -> Result<()> {
    let env = prepare(target, true)?;
    let args: Vec<&str> = if std::env::var("WT_SKIP_FRONTEND").is_ok() {
        vec![
            "run",
            "tauri",
            "android",
            "build",
            "-c",
            "{\"build\":{\"beforeBuildCommand\":\"\"}}",
            "--target",
            target,
            "--apk",
        ]
    } else {
        vec![
            "run", "tauri", "android", "build", "--target", target, "--apk",
        ]
    };
    spawn_with_env("bun", &args, &env)?;
    let apk = apk_release_dir().join("app-universal-release-unsigned.apk");
    if apk.exists() {
        let mb = fs::metadata(&apk)?.len() as f64 / 1024.0 / 1024.0;
        println!("\nAPK: {}\nsize: {:.1} MB", apk.display(), mb);
    }
    Ok(())
}

pub(super) fn cmd_install(target: &str, fresh: bool) -> Result<()> {
    let t0 = Instant::now();
    let mut env = prepare(target, true)?;
    env.push(("WT_DEV_APK".into(), "1".into()));
    spawn_with_env(
        "bun",
        &[
            "run", "tauri", "android", "build", "--target", target, "--apk",
        ],
        &env,
    )?;
    let apk_dir = apk_release_dir();
    let unsigned = apk_dir.join("app-universal-release-unsigned.apk");
    let signed = apk_dir.join("app-universal-release.apk");
    if unsigned.exists() {
        sign_with_debug_keystore(&unsigned, &signed)?;
    } else if !signed.exists() {
        bail!(
            "no APK found at {} or {}",
            unsigned.display(),
            signed.display()
        );
    }
    if fresh {
        println!("\n→ adb uninstall {ANDROID_PACKAGE} (--fresh)");
        let _ = Command::new("adb")
            .args(["uninstall", ANDROID_PACKAGE])
            .status();
    }
    println!("\n→ adb install -r {}", signed.display());
    sh("adb", &["install", "-r", &signed.to_string_lossy()])?;
    let mb = fs::metadata(&signed)?.len() as f64 / 1024.0 / 1024.0;
    println!(
        "\n✓ installed {:.1} MB in {:.1}s{}",
        mb,
        t0.elapsed().as_secs_f64(),
        if fresh {
            " (fresh, models will re-download)"
        } else {
            " (data preserved)"
        }
    );
    Ok(())
}

pub(super) fn cmd_cli(target: &str, debug: bool) -> Result<()> {
    ensure_prebuilts(target)?;
    let abi = abi_for(target)?;
    let env = build_env(target)?;
    let mut cargo_args: Vec<&str> = vec![
        "build",
        "--manifest-path",
        "src-tauri/Cargo.toml",
        "--bin",
        "wt",
        "--target",
        abi.triple,
    ];
    if !debug {
        cargo_args.push("--release");
    }
    spawn_with_env("cargo", &cargo_args, &env)?;
    let bin = root()
        .join("src-tauri")
        .join("target")
        .join(abi.triple)
        .join(if debug { "debug" } else { "release" })
        .join("wt");
    if bin.exists() {
        let mb = fs::metadata(&bin)?.len() as f64 / 1024.0 / 1024.0;
        println!("\nbinary: {}\nsize: {:.1} MB", bin.display(), mb);
    }
    Ok(())
}

pub(super) fn cmd_cli_push() -> Result<()> {
    cmd_cli("aarch64", true)?;
    let bin = root()
        .join("src-tauri")
        .join("target")
        .join("aarch64-linux-android")
        .join("debug")
        .join("wt");
    if !bin.exists() {
        bail!("wt binary missing at {}", bin.display());
    }
    sh(
        "adb",
        &["push", &bin.to_string_lossy(), "/data/local/tmp/wt"],
    )?;
    sh("adb", &["shell", "chmod", "755", "/data/local/tmp/wt"])?;
    println!("pushed to /data/local/tmp/wt");
    Ok(())
}

pub(super) fn cmd_cli_run(args: &[String]) -> Result<()> {
    sh(
        "adb",
        &["shell", &format!("/data/local/tmp/wt {}", args.join(" "))],
    )
}

pub(super) fn cmd_prebuilts(version: Option<String>) -> Result<()> {
    let ver_file = root().join("src-tauri").join("sherpa-version.txt");
    let version = match version {
        Some(v) => v.trim_start_matches('v').to_string(),
        None => fs::read_to_string(&ver_file)
            .with_context(|| format!("read {}", ver_file.display()))?
            .trim()
            .trim_start_matches('v')
            .to_string(),
    };
    let dest = root().join(".android-prebuilt");
    let archive_name = format!("sherpa-onnx-v{version}-android.tar.bz2");
    let url = format!(
        "https://github.com/k2-fsa/sherpa-onnx/releases/download/v{version}/{archive_name}"
    );
    let archive_path = dest.join(&archive_name);
    let marker = dest
        .join("jniLibs")
        .join("arm64-v8a")
        .join("libsherpa-onnx-c-api.so");

    if marker.exists() {
        println!("android prebuilts already present at {}", dest.display());
        return Ok(());
    }
    fs::create_dir_all(&dest)?;

    let need_download = !archive_path.exists()
        || fs::metadata(&archive_path).map(|m| m.len()).unwrap_or(0) < 1_000_000;
    if need_download {
        println!("downloading {url}");
        let status = Command::new("curl")
            .args([
                "-fsSL",
                "--retry",
                "3",
                "-o",
                &archive_path.to_string_lossy(),
                &url,
            ])
            .status()
            .context("spawn curl (curl required for prebuilts download)")?;
        if !status.success() {
            bail!("curl failed for {url}");
        }
        let mb = fs::metadata(&archive_path)?.len() as f64 / 1024.0 / 1024.0;
        println!("  {mb:.1} MB");
    }
    println!("extracting {archive_name}");
    sh_in(
        &dest,
        "tar",
        &["-xjf", &archive_path.file_name().unwrap().to_string_lossy()],
    )
    .context("tar -xjf failed (need a tar that handles bz2; available on Win10+, macOS, Linux)")?;
    fs::write(dest.join(".gitignore"), "*\n")?;
    if !marker.exists() {
        bail!(
            "extraction succeeded but marker missing: {}",
            marker.display()
        );
    }
    println!("android prebuilts staged at {}", dest.display());
    Ok(())
}
