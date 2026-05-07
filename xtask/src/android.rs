use anyhow::{Context, Result, bail};
use clap::{Args as ClapArgs, Subcommand};
use std::fs;
use std::path::{Path, PathBuf};

use crate::util::{exe, root, sh, sh_in};

#[derive(Subcommand)]
#[command(about = "Android build / dev / doctor / prebuilts / sign-patch / cli helpers")]
pub enum Cmd {
    /// Build APK for a target ABI (default aarch64)
    Build(TargetArgs),
    /// Build (debuggable), sign with debug.keystore, adb install -r — fast iteration
    Install(InstallArgs),
    /// Run `tauri android dev` for a target ABI
    Dev(DevArgs),
    /// Print Android toolchain locations and prebuilts status
    Doctor(TargetArgs),
    /// Build the headless `wt` CLI for Android
    Cli(CliArgs),
    /// Push the Android `wt` CLI to a connected device (replaces android-wt.sh push)
    CliPush,
    /// Run `wt` on the connected Android device with arbitrary args
    CliRun {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Download + extract sherpa-onnx Android prebuilts (idempotent)
    Prebuilts {
        #[arg(default_value = "")]
        version: String,
    },
    /// Patch generated build.gradle.kts to enable release signing (idempotent)
    SignPatch,
}

#[derive(ClapArgs)]
pub struct TargetArgs {
    #[arg(long, default_value = "aarch64")]
    pub target: String,
}

#[derive(ClapArgs)]
pub struct InstallArgs {
    #[arg(long, default_value = "aarch64")]
    pub target: String,
    /// adb uninstall before installing (wipes app data, including downloaded models)
    #[arg(long)]
    pub fresh: bool,
}

#[derive(ClapArgs)]
pub struct DevArgs {
    #[arg(long, default_value = "aarch64")]
    pub target: String,
    #[arg(long)]
    pub open: bool,
}

#[derive(ClapArgs)]
pub struct CliArgs {
    #[arg(long, default_value = "aarch64")]
    pub target: String,
    #[arg(long)]
    pub debug: bool,
}

pub fn run(c: Cmd) -> Result<()> {
    match c {
        Cmd::Build(a) => cmd_build(&a.target),
        Cmd::Install(a) => cmd_install(&a.target, a.fresh),
        Cmd::Dev(a) => cmd_dev(&a.target, a.open),
        Cmd::Doctor(a) => cmd_doctor(&a.target),
        Cmd::Cli(a) => cmd_cli(&a.target, a.debug),
        Cmd::CliPush => cmd_cli_push(),
        Cmd::CliRun { args } => cmd_cli_run(&args),
        Cmd::Prebuilts { version } => cmd_prebuilts(if version.is_empty() {
            None
        } else {
            Some(version)
        }),
        Cmd::SignPatch => {
            let _ = sign_patch_inline()?;
            Ok(())
        }
    }
}

struct Abi {
    abi: &'static str,
    rust: &'static str,
    clang: &'static str,
    triple: &'static str,
}

fn abi_for(target: &str) -> Result<Abi> {
    Ok(match target {
        "aarch64" => Abi {
            abi: "arm64-v8a",
            rust: "aarch64_linux_android",
            clang: "aarch64-linux-android24-clang",
            triple: "aarch64-linux-android",
        },
        "armv7" => Abi {
            abi: "armeabi-v7a",
            rust: "armv7_linux_androideabi",
            clang: "armv7a-linux-androideabi24-clang",
            triple: "armv7-linux-androideabi",
        },
        "i686" => Abi {
            abi: "x86",
            rust: "i686_linux_android",
            clang: "i686-linux-android24-clang",
            triple: "i686-linux-android",
        },
        "x86_64" => Abi {
            abi: "x86_64",
            rust: "x86_64_linux_android",
            clang: "x86_64-linux-android24-clang",
            triple: "x86_64-linux-android",
        },
        other => bail!("unknown target: {other} (expected: aarch64|armv7|i686|x86_64)"),
    })
}

fn android_home() -> PathBuf {
    if let Ok(v) = std::env::var("ANDROID_HOME") {
        return PathBuf::from(v);
    }
    if cfg!(target_os = "windows") {
        let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
        PathBuf::from(local).join("Android").join("Sdk")
    } else if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var("HOME").unwrap_or_default())
            .join("Library")
            .join("Android")
            .join("sdk")
    } else {
        PathBuf::from(std::env::var("HOME").unwrap_or_default())
            .join("Android")
            .join("Sdk")
    }
}

fn ndk_home(android_home: &Path) -> PathBuf {
    if let Ok(v) = std::env::var("NDK_HOME") {
        return PathBuf::from(v);
    }
    android_home.join("ndk").join("27.2.12479018")
}

fn ndk_bin(ndk: &Path) -> PathBuf {
    let host = if cfg!(target_os = "windows") {
        "windows-x86_64"
    } else if cfg!(target_os = "macos") {
        "darwin-x86_64"
    } else {
        "linux-x86_64"
    };
    ndk.join("toolchains").join("llvm").join("prebuilt").join(host).join("bin")
}

fn clang_ext() -> &'static str {
    if cfg!(target_os = "windows") { ".cmd" } else { "" }
}

fn prebuilt_dir(target: &str) -> Result<PathBuf> {
    let abi = abi_for(target)?;
    Ok(root().join(".android-prebuilt").join("jniLibs").join(abi.abi))
}

fn cmd_doctor(target: &str) -> Result<()> {
    let abi = abi_for(target)?;
    let sdk = android_home();
    let ndk = ndk_home(&sdk);
    let bin = ndk_bin(&ndk);
    let pdir = prebuilt_dir(target)?;
    let sherpa = pdir.join("libsherpa-onnx-c-api.so");
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

fn ensure_prebuilts(target: &str) -> Result<()> {
    let pdir = prebuilt_dir(target)?;
    if pdir.join("libsherpa-onnx-c-api.so").exists() {
        return Ok(());
    }
    eprintln!("sherpa-onnx prebuilts missing — fetching");
    cmd_prebuilts(None)
}

fn build_env(target: &str) -> Result<Vec<(String, String)>> {
    let abi = abi_for(target)?;
    let sdk = android_home();
    let ndk = ndk_home(&sdk);
    let bin = ndk_bin(&ndk);
    let pdir = prebuilt_dir(target)?;
    let cc = bin.join(format!("{}{}", abi.clang, clang_ext()));
    let cxx = bin.join(format!("{}++{}", abi.clang, clang_ext()));
    let ar = bin.join(exe("llvm-ar"));
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
    ];
    e.sort();
    Ok(e)
}

fn cmd_build(target: &str) -> Result<()> {
    ensure_prebuilts(target)?;
    let _ = sign_patch_inline()?;
    copy_llama_jni(target)?;
    patch_manifest()?;
    let env = build_env(target)?;
    spawn_with_env(
        "bun",
        &["run", "tauri", "android", "build", "--target", target, "--apk"],
        &env,
    )?;
    let abi = abi_for(target)?;
    let apk = root()
        .join("src-tauri")
        .join("gen")
        .join("android")
        .join("app")
        .join("build")
        .join("outputs")
        .join("apk")
        .join("universal")
        .join("release")
        .join("app-universal-release-unsigned.apk");
    let _ = abi;
    if apk.exists() {
        let mb = fs::metadata(&apk)?.len() as f64 / 1024.0 / 1024.0;
        println!("\nAPK: {}\nsize: {:.1} MB", apk.display(), mb);
    }
    Ok(())
}

fn cmd_install(target: &str, fresh: bool) -> Result<()> {
    let t0 = std::time::Instant::now();
    ensure_prebuilts(target)?;
    let _ = sign_patch_inline()?;
    copy_llama_jni(target)?;
    patch_manifest()?;
    let mut env = build_env(target)?;
    env.push(("WT_DEV_APK".to_string(), "1".to_string()));
    spawn_with_env(
        "bun",
        &["run", "tauri", "android", "build", "--target", target, "--apk"],
        &env,
    )?;
    let apk_dir = root()
        .join("src-tauri")
        .join("gen")
        .join("android")
        .join("app")
        .join("build")
        .join("outputs")
        .join("apk")
        .join("universal")
        .join("release");
    let unsigned = apk_dir.join("app-universal-release-unsigned.apk");
    if !unsigned.exists() {
        bail!("unsigned APK not found at {}", unsigned.display());
    }
    let signed = apk_dir.join("app-universal-release.apk");
    sign_with_debug_keystore(&unsigned, &signed)?;
    let signed_str = signed.to_string_lossy().to_string();
    if fresh {
        println!("\n→ adb uninstall com.asolopovas.wtranscriber (--fresh)");
        let _ = std::process::Command::new("adb")
            .args(["uninstall", "com.asolopovas.wtranscriber"])
            .status();
    }
    println!("\n→ adb install -r {}", signed.display());
    sh("adb", &["install", "-r", &signed_str])?;
    let mb = fs::metadata(&signed)?.len() as f64 / 1024.0 / 1024.0;
    println!(
        "\n✓ installed {:.1} MB in {:.1}s{}",
        mb,
        t0.elapsed().as_secs_f64(),
        if fresh { " (fresh, models will re-download)" } else { " (data preserved)" }
    );
    Ok(())
}

fn sign_with_debug_keystore(unsigned: &Path, signed: &Path) -> Result<()> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default();
    let ks = Path::new(&home).join(".android").join("debug.keystore");
    if !ks.exists() {
        bail!("debug.keystore not found at {}", ks.display());
    }
    let sdk = std::env::var("ANDROID_HOME").unwrap_or_else(|_| {
        let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
        format!("{local}\\Android\\Sdk")
    });
    let bt_dir = Path::new(&sdk).join("build-tools");
    let bt_ver = fs::read_dir(&bt_dir)?
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .max()
        .context("no Android build-tools installed")?;
    let bt = bt_dir.join(bt_ver);
    let zipalign = bt.join(exe("zipalign"));
    let apksigner = if cfg!(target_os = "windows") {
        bt.join("apksigner.bat")
    } else {
        bt.join("apksigner")
    };
    let aligned = unsigned.with_file_name("app-universal-release-aligned.apk");
    let aligned_str = aligned.to_string_lossy().to_string();
    let unsigned_str = unsigned.to_string_lossy().to_string();
    let signed_str = signed.to_string_lossy().to_string();
    let ks_str = ks.to_string_lossy().to_string();
    sh(
        zipalign.to_string_lossy().as_ref(),
        &["-f", "-p", "4", &unsigned_str, &aligned_str],
    )?;
    sh(
        apksigner.to_string_lossy().as_ref(),
        &[
            "sign",
            "--ks",
            &ks_str,
            "--ks-pass",
            "pass:android",
            "--ks-key-alias",
            "androiddebugkey",
            "--key-pass",
            "pass:android",
            "--out",
            &signed_str,
            &aligned_str,
        ],
    )?;
    let _ = fs::remove_file(&aligned);
    Ok(())
}

fn cmd_dev(target: &str, open: bool) -> Result<()> {
    ensure_prebuilts(target)?;
    let dev = std::process::Command::new("adb").arg("devices").output()?;
    let txt = String::from_utf8_lossy(&dev.stdout);
    let has_device = txt.lines().any(|l| l.trim().ends_with("\tdevice"));
    if !has_device {
        bail!("no adb device — connect device and enable USB debugging");
    }
    copy_llama_jni(target)?;
    patch_manifest()?;
    let env = build_env(target)?;
    let mut tauri_args: Vec<&str> = vec!["run", "tauri", "android", "dev", "--target", target];
    if open {
        tauri_args.push("--open");
    }
    spawn_with_env("bun", &tauri_args, &env)
}

fn cmd_cli(target: &str, debug: bool) -> Result<()> {
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
    let variant = if debug { "debug" } else { "release" };
    let bin = root()
        .join("src-tauri")
        .join("target")
        .join(abi.triple)
        .join(variant)
        .join(exe("wt"));
    if bin.exists() {
        let mb = fs::metadata(&bin)?.len() as f64 / 1024.0 / 1024.0;
        println!("\nbinary: {}\nsize: {:.1} MB", bin.display(), mb);
    }
    Ok(())
}

fn cmd_cli_push() -> Result<()> {
    cmd_cli("aarch64", true)?;
    let bin = root()
        .join("src-tauri")
        .join("target")
        .join("aarch64-linux-android")
        .join("debug")
        .join(exe("wt"));
    if !bin.exists() {
        bail!("wt binary missing at {}", bin.display());
    }
    sh(
        "adb",
        &[
            "push",
            bin.to_string_lossy().as_ref(),
            "/data/local/tmp/wt",
        ],
    )?;
    sh("adb", &["shell", "chmod", "755", "/data/local/tmp/wt"])?;
    println!("pushed to /data/local/tmp/wt");
    Ok(())
}

fn cmd_cli_run(args: &[String]) -> Result<()> {
    let joined = args.join(" ");
    sh(
        "adb",
        &["shell", &format!("/data/local/tmp/wt {joined}")],
    )
}

fn cmd_prebuilts(version: Option<String>) -> Result<()> {
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
    let marker = dest.join("jniLibs").join("arm64-v8a").join("libsherpa-onnx-c-api.so");

    if marker.exists() {
        println!("android prebuilts already present at {}", dest.display());
        return Ok(());
    }
    fs::create_dir_all(&dest)?;

    let need_download = !archive_path.exists()
        || fs::metadata(&archive_path).map(|m| m.len()).unwrap_or(0) < 1_000_000;
    if need_download {
        println!("downloading {url}");
        download_with_curl(&url, &archive_path)?;
        let mb = fs::metadata(&archive_path)?.len() as f64 / 1024.0 / 1024.0;
        println!("  {mb:.1} MB");
    }
    println!("extracting {archive_name}");
    sh_in(
        &dest,
        "tar",
        &["-xjf", archive_path.file_name().unwrap().to_string_lossy().as_ref()],
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

fn download_with_curl(url: &str, dst: &Path) -> Result<()> {
    let status = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "--retry",
            "3",
            "-o",
            dst.to_string_lossy().as_ref(),
            url,
        ])
        .status()
        .context("spawn curl (curl required for prebuilts download)")?;
    if !status.success() {
        bail!("curl failed for {url}");
    }
    Ok(())
}

fn copy_llama_jni(target: &str) -> Result<()> {
    let abi = abi_for(target)?.abi;
    let llama_src = root()
        .join("src-tauri")
        .join("jniLibs")
        .join(abi)
        .join("libllama-cli.so");
    let gen_dir = root()
        .join("src-tauri")
        .join("gen")
        .join("android")
        .join("app")
        .join("src")
        .join("main")
        .join("jniLibs")
        .join(abi);
    if llama_src.exists() && gen_dir.exists() {
        fs::create_dir_all(&gen_dir)?;
        fs::copy(&llama_src, gen_dir.join("libllama-cli.so"))?;
    }
    Ok(())
}

fn patch_manifest() -> Result<()> {
    let p = root()
        .join("src-tauri")
        .join("gen")
        .join("android")
        .join("app")
        .join("src")
        .join("main")
        .join("AndroidManifest.xml");
    if !p.exists() {
        return Ok(());
    }
    let raw = fs::read_to_string(&p)?;
    if raw.contains("android:extractNativeLibs") {
        return Ok(());
    }
    let patched = raw.replace(
        "<application",
        "<application\n        android:extractNativeLibs=\"true\"",
    );
    if patched != raw {
        fs::write(&p, patched)?;
    }
    Ok(())
}

fn spawn_with_env(prog: &str, args: &[&str], env: &[(String, String)]) -> Result<()> {
    let mut cmd = std::process::Command::new(prog);
    cmd.args(args).current_dir(root());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let status = cmd.status().with_context(|| format!("spawn {prog}"))?;
    if !status.success() {
        bail!("{} {:?} exited with {:?}", prog, args, status.code());
    }
    Ok(())
}

/// Idempotent gradle patch — adds `signingConfigs.release` to the generated
/// `app/build.gradle.kts` so unsigned APKs become signed when
/// `keystore.properties` is present at the gradle parent directory.
pub fn sign_patch_inline() -> Result<i32> {
    let gradle = root()
        .join("src-tauri")
        .join("gen")
        .join("android")
        .join("app")
        .join("build.gradle.kts");
    if !gradle.exists() {
        println!("sign-patch: gen/android not found — run `xtask android prebuilts` + tauri android init first");
        return Ok(0);
    }
    let marker = "// wtranscriber: keystore-signing-patch v1";
    let raw = fs::read_to_string(&gradle)?;
    if raw.contains(marker) {
        println!("sign-patch: already patched");
        return Ok(0);
    }
    let eol = if raw.contains("\r\n") { "\r\n" } else { "\n" };
    let lines: Vec<&str> = raw.split('\n').collect();

    let find_line = |start: usize, predicate: &dyn Fn(&str) -> bool| -> Option<usize> {
        (start..lines.len()).find(|&i| predicate(lines[i].trim_end_matches('\r')))
    };
    let find_block_end = |start: usize| -> Option<usize> {
        let mut depth: i32 = 0;
        let mut seen_open = false;
        for (i, line) in lines.iter().enumerate().skip(start) {
            for ch in line.chars() {
                if ch == '{' {
                    depth += 1;
                    seen_open = true;
                } else if ch == '}' {
                    depth -= 1;
                    if seen_open && depth == 0 {
                        return Some(i);
                    }
                }
            }
        }
        None
    };

    let android_idx = find_line(0, &|l| l.starts_with("android {"));
    let Some(android_idx) = android_idx else {
        println!("sign-patch: `android {{` block not found — skipping");
        return Ok(0);
    };

    // Insert keystoreProperties block right after `android {` line.
    let load_props = vec![
        format!("    {marker}"),
        "    val keystorePropertiesFile = rootProject.file(\"keystore.properties\")".into(),
        "    val keystoreProperties = java.util.Properties()".into(),
        "    if (keystorePropertiesFile.exists()) {".into(),
        "        keystorePropertiesFile.inputStream().use { keystoreProperties.load(it) }".into(),
        "    }".into(),
        "    signingConfigs {".into(),
        "        register(\"release\") {".into(),
        "            if (keystorePropertiesFile.exists()) {".into(),
        "                storeFile = file(keystoreProperties[\"storeFile\"] as String)".into(),
        "                storePassword = keystoreProperties[\"storePassword\"] as String".into(),
        "                keyAlias = keystoreProperties[\"keyAlias\"] as String".into(),
        "                keyPassword = keystoreProperties[\"keyPassword\"] as String".into(),
        "            }".into(),
        "        }".into(),
        "    }".into(),
    ];

    let release_idx = find_line(android_idx, &|l| {
        l.trim().starts_with("getByName(\"release\")") || l.trim().starts_with("release {")
    });
    let mut new_lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();

    new_lines.splice((android_idx + 1)..(android_idx + 1), load_props.clone());
    let shift = load_props.len();
    let release_idx = release_idx.map(|i| i + shift);

    if let Some(rel_idx) = release_idx {
        if let Some(end) = find_block_end(rel_idx) {
            let _ = end;
            let line = new_lines[rel_idx].clone();
            if !line.contains("signingConfig") {
                let inject = "            signingConfig = signingConfigs.getByName(\"release\")";
                new_lines.insert(rel_idx + 1, inject.to_string());
            }
        }
    }
    let mut joined = new_lines.join("\n");
    if eol == "\r\n" {
        joined = joined.replace('\n', "\r\n");
    }
    fs::write(&gradle, joined)?;
    println!("sign-patch: applied");
    Ok(0)
}
