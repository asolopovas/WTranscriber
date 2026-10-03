use anyhow::{Context, Result, bail};
use std::fs;
use std::process::Command;

use crate::util::{exe, parallel_build_env, parallel_jobs, root, sh_in};

use super::patch::{
    copy_jni_prebuilts, patch_generated_activities, patch_gradle_build_config,
    patch_gradle_properties, patch_manifest, sign_patch_inline,
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
        ("CARGO_INCREMENTAL".into(), "0".into()),
        ("ANDROID_HOME".into(), sdk.display().to_string()),
        ("NDK_HOME".into(), ndk.display().to_string()),
        ("SHERPA_ONNX_LIB_DIR".into(), pdir.display().to_string()),
        ("ORT_STRATEGY".into(), "system".into()),
        ("ORT_LIB_LOCATION".into(), pdir.display().to_string()),
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
    e.extend(parallel_build_env(parallel_jobs()));
    e.sort();
    Ok(e)
}

pub(super) fn ensure_prebuilts(target: &str) -> Result<()> {
    let pdir = prebuilt_dir(target)?;
    if pdir.join("libsherpa-onnx-c-api.so").exists()
        && pdir.join("libonnxruntime.so").exists()
        && prebuilt_version_matches(&root().join(".android-prebuilt"), sherpa_version())
    {
        return Ok(());
    }
    eprintln!("sherpa-onnx prebuilts missing or outdated — fetching");
    cmd_prebuilts(None)
}

pub(super) fn prepare(target: &str, with_sign: bool) -> Result<Vec<(String, String)>> {
    ensure_prebuilts(target)?;
    if with_sign {
        sign_patch_inline()?;
    }
    patch_gradle_build_config()?;
    patch_gradle_properties()?;
    patch_generated_activities()?;
    purge_other_jni_abis(target)?;
    copy_jni_prebuilts(target)?;
    patch_manifest()?;
    crate::release::ensure_dev_keystore_properties(true)?;
    build_env(target)
}

fn purge_other_jni_abis(target: &str) -> Result<()> {
    let keep = abi_for(target)?.abi;
    let jni = root().join("src-tauri/gen/android/app/src/main/jniLibs");
    if !jni.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(&jni)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        if entry.file_name().to_string_lossy() == keep {
            continue;
        }
        let _ = fs::remove_dir_all(entry.path());
    }
    Ok(())
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
            "--",
            "--no-default-features",
            "--features",
            "android",
        ]
    } else {
        vec![
            "run",
            "tauri",
            "android",
            "build",
            "--target",
            target,
            "--apk",
            "--",
            "--no-default-features",
            "--features",
            "android",
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

pub(super) fn cmd_prebuilts(version: Option<String>) -> Result<()> {
    let version = version.map_or_else(
        || sherpa_version().to_string(),
        |v| v.trim().trim_start_matches('v').to_string(),
    );
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

    if marker.exists()
        && marker.with_file_name("libonnxruntime.so").exists()
        && prebuilt_version_matches(&dest, &version)
    {
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
    let version_stamp = dest.join("version.txt");
    if version_stamp.exists() {
        fs::remove_file(&version_stamp)?;
    }
    let staging = dest.join(format!("staging-{version}"));
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;
    println!("extracting {archive_name}");
    sh_in(&staging, "tar", &["-xjf", &archive_path.to_string_lossy()]).context(
        "tar -xjf failed (need a tar that handles bz2; available on Win10+, macOS, Linux)",
    )?;
    let staged_libraries = staging.join("jniLibs");
    let staged_marker = staged_libraries
        .join("arm64-v8a")
        .join("libsherpa-onnx-c-api.so");
    if !staged_marker.exists() || !staged_marker.with_file_name("libonnxruntime.so").exists() {
        bail!("extraction did not contain the expected sherpa-onnx and ONNX Runtime libraries");
    }
    let libraries = dest.join("jniLibs");
    if libraries.exists() {
        fs::remove_dir_all(&libraries)?;
    }
    fs::rename(staged_libraries, libraries)?;
    fs::remove_dir_all(staging)?;
    fs::write(dest.join(".gitignore"), "*\n")?;
    fs::write(version_stamp, &version)?;
    println!("android prebuilts staged at {}", dest.display());
    Ok(())
}

fn sherpa_version() -> &'static str {
    include_str!("../../../src-tauri/sherpa-version.txt")
        .trim()
        .trim_start_matches('v')
}

fn prebuilt_version_matches(directory: &std::path::Path, version: &str) -> bool {
    fs::read_to_string(directory.join("version.txt")).is_ok_and(|cached| cached.trim() == version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prebuilt_cache_requires_matching_version() {
        let directory = std::env::temp_dir().join(format!(
            "wtranscriber-prebuilt-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        assert!(!prebuilt_version_matches(&directory, sherpa_version()));
        fs::write(directory.join("version.txt"), "1.13.2").unwrap();
        assert!(!prebuilt_version_matches(&directory, sherpa_version()));
        fs::write(
            directory.join("version.txt"),
            format!("{}\n", sherpa_version()),
        )
        .unwrap();
        assert!(prebuilt_version_matches(&directory, sherpa_version()));
        fs::remove_dir_all(directory).unwrap();
    }
}
