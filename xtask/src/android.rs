use anyhow::{Context, Result, bail};
use clap::{Args as ClapArgs, Subcommand, ValueEnum};
use serde_json::json;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::util::{exe, root, sh, sh_in};

const ANDROID_PACKAGE: &str = "com.asolopovas.wtranscriber";

#[derive(Subcommand)]
#[command(about = "Android build / dev / doctor / prebuilts / sign-patch / cli helpers")]
pub enum Cmd {
    Build(TargetArgs),
    Install(InstallArgs),
    Dev(DevArgs),
    Bootstrap(BootstrapArgs),
    Status(StatusArgs),
    Stop(StopArgs),
    Attach(AttachArgs),
    Smoke(AttachArgs),
    Doctor(TargetArgs),
    Cli(CliArgs),
    CliPush,
    CliRun {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    Prebuilts {
        #[arg(default_value = "")]
        version: String,
    },
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
    #[arg(long)]
    pub fresh: bool,
}

#[derive(ClapArgs)]
pub struct DevArgs {
    #[arg(long)]
    pub open: bool,
    #[arg(long)]
    pub host: bool,
    #[arg(long)]
    pub watch: bool,
    pub device: Option<String>,
}

#[derive(Clone, ValueEnum)]
pub enum BootstrapMode {
    Usb,
    Host,
}

#[derive(ClapArgs)]
pub struct BootstrapArgs {
    #[arg(value_enum, default_value_t = BootstrapMode::Usb)]
    pub mode: BootstrapMode,
    pub device: Option<String>,
}

#[derive(ClapArgs)]
pub struct StatusArgs {
    #[arg(long)]
    pub json: bool,
    pub device: Option<String>,
}

#[derive(ClapArgs)]
pub struct StopArgs {
    #[arg(long)]
    pub keep_reverse: bool,
    pub device: Option<String>,
}

#[derive(ClapArgs)]
pub struct AttachArgs {
    pub device: Option<String>,
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
        Cmd::Dev(a) => cmd_dev(a.open, a.host, a.watch, a.device.as_deref()),
        Cmd::Bootstrap(a) => cmd_bootstrap(a.mode, a.device.as_deref()),
        Cmd::Status(a) => cmd_status(a.json, a.device.as_deref()),
        Cmd::Stop(a) => cmd_stop(a.keep_reverse, a.device.as_deref()),
        Cmd::Attach(a) => attach_webview(a.device.as_deref(), false),
        Cmd::Smoke(a) => cmd_smoke(a.device.as_deref()),
        Cmd::Doctor(a) => cmd_doctor(&a.target),
        Cmd::Cli(a) => cmd_cli(&a.target, a.debug),
        Cmd::CliPush => cmd_cli_push(),
        Cmd::CliRun { args } => cmd_cli_run(&args),
        Cmd::Prebuilts { version } => cmd_prebuilts((!version.is_empty()).then_some(version)),
        Cmd::SignPatch => sign_patch_inline().map(|_| ()),
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
        PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_default())
            .join("Android")
            .join("Sdk")
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
    ndk.join("toolchains")
        .join("llvm")
        .join("prebuilt")
        .join(host)
        .join("bin")
}

fn clang_ext() -> &'static str {
    if cfg!(target_os = "windows") {
        ".cmd"
    } else {
        ""
    }
}

fn gen_android() -> PathBuf {
    root().join("src-tauri").join("gen").join("android")
}

fn apk_release_dir() -> PathBuf {
    gen_android()
        .join("app")
        .join("build")
        .join("outputs")
        .join("apk")
        .join("universal")
        .join("release")
}

fn prebuilt_dir(target: &str) -> Result<PathBuf> {
    Ok(root()
        .join(".android-prebuilt")
        .join("jniLibs")
        .join(abi_for(target)?.abi))
}

fn wait_output(mut child: Child, timeout: Duration) -> Option<Output> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().ok(),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(_) => return None,
        }
    }
}

fn run_timeout(prog: &str, args: &[&str], timeout: Duration) -> Result<()> {
    let child = Command::new(prog)
        .args(args)
        .current_dir(root())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawn {prog}"))?;
    match wait_output(child, timeout) {
        Some(out) if out.status.success() => Ok(()),
        Some(out) => bail!(
            "{} {:?} failed: {}",
            prog,
            args,
            String::from_utf8_lossy(&out.stderr).trim()
        ),
        None => bail!("{} {:?} timed out after {}s", prog, args, timeout.as_secs()),
    }
}

fn capture_timeout(prog: &str, args: &[&str], timeout: Duration) -> Option<String> {
    let child = Command::new(prog)
        .args(args)
        .current_dir(root())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let out = wait_output(child, timeout)?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn with_device<'a>(device: Option<&'a str>, args: &[&'a str]) -> Vec<&'a str> {
    let mut all = Vec::with_capacity(args.len() + 2);
    if let Some(d) = device {
        all.push("-s");
        all.push(d);
    }
    all.extend_from_slice(args);
    all
}

fn adb_run(device: Option<&str>, args: &[&str], timeout: Duration) -> Result<()> {
    run_timeout("adb", &with_device(device, args), timeout)
}

fn adb_capture(device: Option<&str>, args: &[&str], timeout: Duration) -> Option<String> {
    capture_timeout("adb", &with_device(device, args), timeout)
}

fn adb_reverse(device: Option<&str>, port: &str) -> Result<()> {
    let spec = format!("tcp:{port}");
    adb_run(device, &["reverse", &spec, &spec], Duration::from_secs(5))
}

fn spawn_detached(
    prog: &str,
    args: &[&str],
    env: &[(String, String)],
    stdout_path: &Path,
    stderr_path: &Path,
) -> Result<u32> {
    if let Some(parent) = stdout_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut cmd = Command::new(prog);
    cmd.args(args)
        .current_dir(root())
        .stdin(Stdio::null())
        .stdout(Stdio::from(File::create(stdout_path)?))
        .stderr(Stdio::from(File::create(stderr_path)?));
    for (k, v) in env {
        cmd.env(k, v);
    }
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);
    Ok(cmd.spawn().with_context(|| format!("spawn {prog}"))?.id())
}

fn spawn_with_env(prog: &str, args: &[&str], env: &[(String, String)]) -> Result<()> {
    let mut cmd = Command::new(prog);
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

fn port_owner(port: u16) -> Option<u32> {
    if !cfg!(windows) {
        return None;
    }
    let pattern = format!(":{port}");
    let out = capture_timeout("netstat", &["-ano"], Duration::from_secs(2))?;
    out.lines()
        .find(|line| line.contains(&pattern) && line.contains("LISTENING"))
        .and_then(|line| line.split_whitespace().last())
        .and_then(|pid| pid.parse::<u32>().ok())
}

fn tcp_open(port: u16) -> bool {
    TcpStream::connect_timeout(
        &std::net::SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, port)),
        Duration::from_millis(250),
    )
    .is_ok()
}

fn wait_for_port(port: u16, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if tcp_open(port) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(500));
    }
    bail!("vite did not bind :{port} within {}s", timeout.as_secs())
}

fn last_line_matching(path: &Path, f: impl Fn(&str) -> bool) -> Option<String> {
    fs::read_to_string(path)
        .ok()?
        .lines()
        .rev()
        .take(500)
        .find(|line| f(line))
        .map(str::to_string)
}

fn tail_any(paths: &[&Path], f: impl Fn(&str) -> bool) -> bool {
    paths.iter().any(|p| last_line_matching(p, &f).is_some())
}

fn wait_for_log_line(
    paths: &[&Path],
    label: &str,
    f: impl Fn(&str) -> bool,
    timeout: Duration,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if tail_any(paths, &f) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(500));
    }
    bail!(
        "{label} not seen in {paths:?} within {}s — check adb reverse / TAURI_DEV_HOST / device app launch",
        timeout.as_secs()
    )
}

fn file_age_seconds(path: &Path) -> Option<u64> {
    Some(
        fs::metadata(path)
            .ok()?
            .modified()
            .ok()?
            .elapsed()
            .ok()?
            .as_secs(),
    )
}

fn json_seconds(value: &serde_json::Value) -> String {
    value.as_u64().map_or("-".into(), |v| v.to_string())
}

fn api_probe(timeout: Duration) -> Option<String> {
    let expr = concat!(
        "import('/src/api.ts').then(m => Promise.all([",
        "m.api.appVersion(), m.api.systemInfo(), m.api.loadConfig()",
        "]).then(([version, systemInfo]) => ({version, os: systemInfo.os, ok: true})))"
    );
    capture_timeout("node", &["scripts/cdp.mjs", expr], timeout)
}

fn is_app_crash_signal(line: &str) -> bool {
    line.contains(ANDROID_PACKAGE)
        && (line.contains("am_crash")
            || line.contains("am_proc_died")
            || (line.contains("am_kill")
                && !line.contains("installPackageLI")
                && !line.contains("due to install")))
}

fn read_pids(path: &Path) -> BTreeMap<String, u32> {
    let Ok(raw) = fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return BTreeMap::new();
    };
    value
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(k, v)| Some((k.clone(), u32::try_from(v.as_u64()?).ok()?)))
        .collect()
}

fn pids_device(path: &Path) -> Option<String> {
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str::<serde_json::Value>(&raw)
        .ok()?
        .get("device")?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn pid_alive(pid: u32) -> bool {
    let pid_text = pid.to_string();
    if cfg!(windows) {
        capture_timeout(
            "tasklist",
            &["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"],
            Duration::from_secs(2),
        )
        .is_some_and(|out| out.contains(&pid_text))
    } else {
        Command::new("kill")
            .args(["-0", &pid_text])
            .status()
            .is_ok_and(|s| s.success())
    }
}

fn kill_pid(pid: u32) {
    let pid_text = pid.to_string();
    if cfg!(windows) {
        let _ = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid_text])
            .status();
    } else {
        let _ = Command::new("kill").args(["-TERM", &pid_text]).status();
    }
}

fn adb_devices() -> Vec<String> {
    capture_timeout("adb", &["devices"], Duration::from_secs(2))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(serial, state)| format!("{}:{}", serial.trim(), state.trim()))
        .collect()
}

fn preflight_node_modules() -> Result<()> {
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

fn reap_tauri_logcat_orphans() {
    if !cfg!(windows) {
        return;
    }
    let Some(out) = capture_timeout(
        "powershell",
        &[
            "-NoProfile",
            "-Command",
            "Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'adb.exe' -and $_.CommandLine -match 'logcat .* -s wtranscriber' } | ForEach-Object { $_.ProcessId }",
        ],
        Duration::from_secs(3),
    ) else {
        return;
    };
    for pid in out.lines().filter_map(|l| l.trim().parse::<u32>().ok()) {
        kill_pid(pid);
        eprintln!("reaped orphan tauri logcat pid={pid}");
    }
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

fn ensure_prebuilts(target: &str) -> Result<()> {
    let pdir = prebuilt_dir(target)?;
    if pdir.join("libsherpa-onnx-c-api.so").exists() {
        return Ok(());
    }
    eprintln!("sherpa-onnx prebuilts missing — fetching");
    cmd_prebuilts(None)
}

fn prepare(target: &str, with_sign: bool) -> Result<Vec<(String, String)>> {
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

fn cmd_doctor(target: &str) -> Result<()> {
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

fn cmd_build(target: &str) -> Result<()> {
    let env = prepare(target, true)?;
    spawn_with_env(
        "bun",
        &[
            "run", "tauri", "android", "build", "--target", target, "--apk",
        ],
        &env,
    )?;
    let apk = apk_release_dir().join("app-universal-release-unsigned.apk");
    if apk.exists() {
        let mb = fs::metadata(&apk)?.len() as f64 / 1024.0 / 1024.0;
        println!("\nAPK: {}\nsize: {:.1} MB", apk.display(), mb);
    }
    Ok(())
}

fn cmd_install(target: &str, fresh: bool) -> Result<()> {
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
    if !unsigned.exists() {
        bail!("unsigned APK not found at {}", unsigned.display());
    }
    let signed = apk_dir.join("app-universal-release.apk");
    sign_with_debug_keystore(&unsigned, &signed)?;
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

fn patch_gradle_properties() -> Result<()> {
    let path = gen_android().join("gradle.properties");
    if !path.exists() {
        return Ok(());
    }
    let raw = fs::read_to_string(&path)?;
    let mut next = raw
        .lines()
        .filter(|line| {
            let line = line.trim_start();
            !line.starts_with("org.gradle.configureondemand=")
                && !line.starts_with("org.gradle.warning.mode=")
                && !line.starts_with("org.gradle.problems.report=")
        })
        .collect::<Vec<_>>()
        .join("\n");
    next.push_str("\norg.gradle.warning.mode=none\norg.gradle.problems.report=false\n");
    if next != raw {
        fs::write(path, next)?;
    }
    Ok(())
}

fn patch_gradle_build_config() -> Result<()> {
    let gradle = gen_android().join("app").join("build.gradle.kts");
    if !gradle.exists() {
        return Ok(());
    }
    let mut raw = fs::read_to_string(&gradle)?;
    if !raw.contains("val wtDevApk =") {
        raw = raw.replace(
            "val tauriProperties = Properties().apply {\n    val propFile = file(\"tauri.properties\")\n    if (propFile.exists()) {\n        propFile.inputStream().use { load(it) }\n    }\n}",
            "val tauriProperties = Properties().apply {\n    val propFile = file(\"tauri.properties\")\n    if (propFile.exists()) {\n        propFile.inputStream().use { load(it) }\n    }\n}\n\nval wtDevApk = (project.findProperty(\"wtDevApk\") as? String == \"true\") || (System.getenv(\"WT_DEV_APK\") == \"1\")",
        );
    }
    raw = raw.replace(
        "isDebuggable = (project.findProperty(\"wtDevApk\") as? String == \"true\") || (System.getenv(\"WT_DEV_APK\") == \"1\")",
        "isDebuggable = wtDevApk",
    );
    raw = raw.replace("isMinifyEnabled = true", "isMinifyEnabled = !wtDevApk");
    if !raw.contains("sourceCompatibility = JavaVersion.VERSION_17") {
        raw = raw.replace(
            "    kotlinOptions {\n        jvmTarget = \"1.8\"\n    }",
            "    compileOptions {\n        sourceCompatibility = JavaVersion.VERSION_17\n        targetCompatibility = JavaVersion.VERSION_17\n    }\n    kotlinOptions {\n        jvmTarget = \"17\"\n        suppressWarnings = true\n    }",
        );
    }
    if raw.contains("jvmTarget = \"17\"") && !raw.contains("suppressWarnings = true") {
        raw = raw.replace(
            "        jvmTarget = \"17\"",
            "        jvmTarget = \"17\"\n        suppressWarnings = true",
        );
    }
    if !raw.contains("jniLibs.useLegacyPackaging = true") {
        raw = raw.replace(
            "    buildFeatures {\n        buildConfig = true\n    }",
            "    buildFeatures {\n        buildConfig = true\n    }\n    packaging {\n        jniLibs.useLegacyPackaging = true\n    }",
        );
    }
    fs::write(&gradle, raw)?;
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
    let bt_dir = android_home().join("build-tools");
    let bt_ver = fs::read_dir(&bt_dir)?
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .max()
        .context("no Android build-tools installed")?;
    let bt = bt_dir.join(bt_ver);
    let zipalign = bt.join(exe("zipalign"));
    let apksigner = bt.join(if cfg!(windows) {
        "apksigner.bat"
    } else {
        "apksigner"
    });
    let aligned = unsigned.with_file_name("app-universal-release-aligned.apk");
    sh(
        &zipalign.to_string_lossy(),
        &[
            "-f",
            "-p",
            "4",
            &unsigned.to_string_lossy(),
            &aligned.to_string_lossy(),
        ],
    )?;
    sh(
        &apksigner.to_string_lossy(),
        &[
            "sign",
            "--ks",
            &ks.to_string_lossy(),
            "--ks-pass",
            "pass:android",
            "--ks-key-alias",
            "androiddebugkey",
            "--key-pass",
            "pass:android",
            "--out",
            &signed.to_string_lossy(),
            &aligned.to_string_lossy(),
        ],
    )?;
    let _ = fs::remove_file(&aligned);
    Ok(())
}

fn cmd_bootstrap(mode: BootstrapMode, device: Option<&str>) -> Result<()> {
    let tmp = root().join("tmp");
    fs::create_dir_all(&tmp)?;
    let pids_path = tmp.join("_pids.json");
    if pids_path.exists() {
        eprintln!("[stage 0/6] stopping previous dev session");
        cmd_stop(false, device)?;
    } else if tcp_open(1420) {
        bail!(
            "port 1420 is already in use; stop the existing dev server before bootstrapping Android"
        );
    }
    reap_tauri_logcat_orphans();
    eprintln!("[stage 1/6] preflight (node_modules, device)");
    preflight_node_modules()?;
    detect_device_target(device)?;
    fs::write(tmp.join("_platform"), "android")?;

    let _ = adb_run(device, &["logcat", "-c"], Duration::from_secs(5));
    let logcat_args: Vec<String> = with_device(
        device,
        &[
            "logcat",
            "-b",
            "main,events",
            "*:W",
            "RustStdoutStderr:V",
            "Tauri:V",
            "chromium:V",
            "am_crash:V",
            "am_proc_died:V",
            "am_kill:V",
        ],
    )
    .into_iter()
    .map(String::from)
    .collect();
    let logcat_arg_refs: Vec<&str> = logcat_args.iter().map(String::as_str).collect();
    eprintln!("[stage 2/6] starting logcat capture");
    let logcat_pid = spawn_detached(
        "adb",
        &logcat_arg_refs,
        &[],
        &tmp.join("logcat.log"),
        &tmp.join("logcat.err.log"),
    )?;

    let mut env = Vec::<(String, String)>::new();
    let mut args = vec![
        "xtask".to_string(),
        "android".to_string(),
        "dev".to_string(),
    ];
    match mode {
        BootstrapMode::Usb => {
            env.push(("TAURI_DEV_HOST".into(), "127.0.0.1".into()));
            adb_reverse(device, "1420")?;
            adb_reverse(device, "1421")?;
        }
        BootstrapMode::Host => args.push("--host".into()),
    }
    if let Some(d) = device {
        args.push(d.to_string());
    }
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    eprintln!("[stage 3/6] spawning tauri android dev (logs: tmp/android-dev.{{log,err.log}})");
    let dev_pid = spawn_detached(
        "cargo",
        &arg_refs,
        &env,
        &tmp.join("android-dev.log"),
        &tmp.join("android-dev.err.log"),
    )?;

    let dev_log = tmp.join("android-dev.log");
    let dev_err = tmp.join("android-dev.err.log");
    let bring_up = || -> Result<()> {
        eprintln!("[stage 4/6] waiting for vite to bind :1420 (≤300s, includes rust compile)");
        wait_for_port(1420, Duration::from_secs(300))?;
        eprintln!(
            "[stage 5/6] waiting for WebView → :1420 connection (≤360s, includes apk build/install/launch)"
        );
        wait_for_log_line(
            &[&dev_log, &dev_err],
            "WebView connecting to :1420",
            |s| {
                (s.contains("connecting to ") || s.contains("connected to ")) && s.contains(":1420")
            },
            Duration::from_secs(360),
        )?;
        eprintln!("[stage 6/6] attaching CDP and probing tauri IPC");
        wait_for_attach(device, Duration::from_secs(30))?;
        api_probe(Duration::from_secs(15))
            .context("Tauri IPC API probe failed after CDP attach")?;
        Ok(())
    };
    if let Err(err) = bring_up() {
        eprintln!("bootstrap failed: {err:#}");
        eprintln!("--- last 10 lines of android-dev.err.log ---");
        if let Ok(raw) = fs::read_to_string(&dev_err) {
            for line in raw.lines().rev().take(10).collect::<Vec<_>>().iter().rev() {
                eprintln!("  {line}");
            }
        }
        kill_pid(dev_pid);
        kill_pid(logcat_pid);
        reap_tauri_logcat_orphans();
        let _ = adb_run(
            device,
            &["forward", "--remove", "tcp:9222"],
            Duration::from_secs(3),
        );
        let _ = adb_run(
            device,
            &["reverse", "--remove", "tcp:1420"],
            Duration::from_secs(3),
        );
        let _ = adb_run(
            device,
            &["reverse", "--remove", "tcp:1421"],
            Duration::from_secs(3),
        );
        let _ = fs::remove_file(tmp.join("_platform"));
        return Err(err);
    }
    let pids = json!({
        "device": device,
        "dev_wrapper": dev_pid,
        "dev_port_owner": port_owner(1420),
        "logcat": logcat_pid
    });
    fs::write(tmp.join("_pids.json"), serde_json::to_string(&pids)?)?;
    println!(
        "BOOTSTRAP OK platform=android mode={} pids={}",
        match mode {
            BootstrapMode::Usb => "usb",
            BootstrapMode::Host => "host",
        },
        pids
    );
    Ok(())
}

fn cmd_status(as_json: bool, device_arg: Option<&str>) -> Result<()> {
    let tmp = root().join("tmp");
    let platform = fs::read_to_string(tmp.join("_platform")).unwrap_or_else(|_| "unknown".into());
    let pids_path = tmp.join("_pids.json");
    let pids = read_pids(&pids_path);
    let device = device_arg
        .map(str::to_string)
        .or_else(|| pids_device(&pids_path));
    let devices = adb_devices();
    let reverse = adb_capture(
        device.as_deref(),
        &["reverse", "--list"],
        Duration::from_secs(2),
    )
    .unwrap_or_default();
    let mut cdp_forward = tcp_open(9222);
    if !cdp_forward {
        let _ = attach_webview(device.as_deref(), as_json);
        cdp_forward = tcp_open(9222);
    }
    let mut api = cdp_forward
        .then(|| api_probe(Duration::from_secs(8)))
        .flatten();
    if api.is_none() {
        let _ = attach_webview(device.as_deref(), as_json);
        cdp_forward = tcp_open(9222);
        api = cdp_forward
            .then(|| api_probe(Duration::from_secs(8)))
            .flatten();
    }
    let reverse1420 = reverse.contains("tcp:1420");
    let reverse1421 = reverse.contains("tcp:1421");
    let vite_alive = tcp_open(1420);
    let api_responsive = api.is_some();
    let session_healthy = vite_alive && reverse1420 && reverse1421 && cdp_forward && api_responsive;
    let dev_log = tmp.join("android-dev.log");
    let dev_err = tmp.join("android-dev.err.log");
    let logcat_log = tmp.join("logcat.log");
    let status = json!({
        "platform": platform.trim(),
        "sessionHealthy": session_healthy,
        "pidsFile": !pids.is_empty(),
        "devWrapperPid": pids.get("dev_wrapper"),
        "devWrapperAlive": pids.get("dev_wrapper").is_some_and(|pid| pid_alive(*pid)),
        "device": device,
        "devPortOwner": pids.get("dev_port_owner"),
        "port1420Owner": port_owner(1420),
        "viteAlive": vite_alive,
        "android": {
            "adbDevices": devices,
            "reverse1420": reverse1420,
            "reverse1421": reverse1421,
            "cdpForward": cdp_forward,
            "apiResponsive": api_responsive,
            "apiProbe": api,
            "devLogAgeSeconds": file_age_seconds(&dev_log),
            "logcatAgeSeconds": file_age_seconds(&logcat_log),
            "recentViteConnection": tail_any(&[&dev_log, &dev_err], |s| (s.contains("connecting to ") || s.contains("connected to ")) && s.contains(":1420")),
            "lastHmrUpdate": last_line_matching(&dev_log, |s| s.contains("[vite] hmr update"))
                .or_else(|| last_line_matching(&dev_err, |s| s.contains("[vite] hmr update"))),
            "lastCrashSignal": last_line_matching(&logcat_log, is_app_crash_signal)
        }
    });
    if as_json {
        println!("{}", serde_json::to_string_pretty(&status)?);
        return Ok(());
    }
    let android = &status["android"];
    println!(
        "platform={} healthy={} pidsFile={} vite=:1420/{} owner={}",
        status["platform"].as_str().unwrap_or("unknown"),
        status["sessionHealthy"].as_bool().unwrap_or(false),
        status["pidsFile"].as_bool().unwrap_or(false),
        status["viteAlive"].as_bool().unwrap_or(false),
        status["port1420Owner"]
            .as_u64()
            .map_or("-".to_string(), |p| p.to_string())
    );
    println!(
        "adbDevices={} reverse1420={} reverse1421={} cdp={} api={}",
        android["adbDevices"]
            .as_array()
            .map_or(String::new(), |items| items
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join(",")),
        android["reverse1420"].as_bool().unwrap_or(false),
        android["reverse1421"].as_bool().unwrap_or(false),
        android["cdpForward"].as_bool().unwrap_or(false),
        android["apiResponsive"].as_bool().unwrap_or(false)
    );
    println!(
        "devLogAge={}s logcatAge={}s viteConnectionInTail={}",
        json_seconds(&android["devLogAgeSeconds"]),
        json_seconds(&android["logcatAgeSeconds"]),
        android["recentViteConnection"].as_bool().unwrap_or(false)
    );
    if let Some(line) = android["lastHmrUpdate"].as_str() {
        println!("lastHmr={line}");
    }
    if let Some(line) = android["lastCrashSignal"].as_str() {
        println!("lastCrash={line}");
    }
    Ok(())
}

fn cmd_stop(keep_reverse: bool, device_arg: Option<&str>) -> Result<()> {
    let tmp = root().join("tmp");
    let pids_path = tmp.join("_pids.json");
    let pids = read_pids(&pids_path);
    let device = device_arg
        .map(str::to_string)
        .or_else(|| pids_device(&pids_path));
    for key in ["logcat", "dev_wrapper", "dev_port_owner"] {
        if let Some(pid) = pids.get(key) {
            kill_pid(*pid);
            println!("stopped {key} pid={pid}");
        }
    }
    reap_tauri_logcat_orphans();
    if !keep_reverse {
        let d = device.as_deref();
        let t = Duration::from_secs(3);
        let _ = adb_run(d, &["forward", "--remove", "tcp:9222"], t);
        let _ = adb_run(d, &["reverse", "--remove", "tcp:1420"], t);
        let _ = adb_run(d, &["reverse", "--remove", "tcp:1421"], t);
    }
    let _ = fs::remove_file(tmp.join("_pids.json"));
    let _ = fs::remove_file(tmp.join("_platform"));
    println!("dev session stopped");
    Ok(())
}

fn attach_webview(device: Option<&str>, quiet: bool) -> Result<()> {
    let socket = adb_capture(
        device,
        &[
            "shell",
            "cat /proc/net/unix | grep -oE 'webview_devtools_remote_[0-9]+' | head -1",
        ],
        Duration::from_secs(10),
    )
    .unwrap_or_default();
    let pid = socket
        .trim()
        .strip_prefix("webview_devtools_remote_")
        .and_then(|s| s.parse::<u32>().ok())
        .context("no WebView devtools socket; is the app running?")?;
    let t = Duration::from_secs(3);
    let _ = adb_run(device, &["forward", "--remove", "tcp:9222"], t);
    let target = format!("localabstract:webview_devtools_remote_{pid}");
    adb_run(device, &["forward", "tcp:9222", &target], t)?;
    if !quiet {
        println!("forwarded tcp:9222 -> webview_devtools_remote_{pid}");
    }
    Ok(())
}

fn wait_for_attach(device: Option<&str>, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        match attach_webview(device, false) {
            Ok(()) => return Ok(()),
            Err(err) if Instant::now() >= deadline => return Err(err),
            Err(_) => thread::sleep(Duration::from_millis(500)),
        }
    }
}

fn cmd_smoke(device: Option<&str>) -> Result<()> {
    let target = detect_device_target(device)?;
    wait_for_port(1420, Duration::from_secs(5))?;
    wait_for_attach(device, Duration::from_secs(10))?;
    let probe = api_probe(Duration::from_secs(10)).context("Tauri IPC API probe failed")?;
    println!("android smoke ok target={target} api={probe}");
    Ok(())
}

fn cmd_dev(open: bool, host: bool, watch: bool, device: Option<&str>) -> Result<()> {
    let target = detect_device_target(device)?;
    println!("android dev: detected ABI → target={target}");
    let mut env = prepare(&target, false)?;
    if std::env::var_os("TAURI_DEV_HOST").is_none()
        && let Some(ip) = detect_dev_host(device)
    {
        println!("android dev: auto-detected TAURI_DEV_HOST={ip}");
        env.push(("TAURI_DEV_HOST".into(), ip));
    }
    let dev_host_arg = std::env::var("TAURI_DEV_HOST").ok();
    let mut tauri_args: Vec<&str> = vec!["run", "tauri", "android", "dev"];
    if open {
        tauri_args.push("--open");
    }
    if host {
        tauri_args.push("--host");
    } else if let Some(ref value) = dev_host_arg {
        tauri_args.push("--host");
        tauri_args.push(value);
    }
    if !watch {
        tauri_args.push("--no-watch");
    }
    if let Some(d) = device {
        tauri_args.push(d);
    }
    spawn_with_env("bun", &tauri_args, &env)
}

fn detect_dev_host(device: Option<&str>) -> Option<String> {
    let txt = adb_capture(
        device,
        &["shell", "ip", "-4", "addr", "show", "wlan0"],
        Duration::from_secs(3),
    )?;
    let device_ip = txt
        .lines()
        .find_map(|l| l.trim().strip_prefix("inet "))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|cidr| cidr.split('/').next())?
        .to_string();
    let mut octets = device_ip.split('.');
    let a = octets.next()?;
    let b = octets.next()?;
    let c = octets.next()?;
    octets.next()?;
    let prefix = format!("{a}.{b}.{c}.");
    host_ipv4_addresses()
        .into_iter()
        .find(|ip| ip.starts_with(&prefix) && ip != &device_ip)
}

fn host_ipv4_addresses() -> Vec<String> {
    let (prog, args): (&str, &[&str]) = if cfg!(windows) {
        (
            "powershell",
            &[
                "-NoProfile",
                "-Command",
                "Get-NetIPAddress -AddressFamily IPv4 -PrefixOrigin Dhcp,Manual -ErrorAction SilentlyContinue | Select-Object -ExpandProperty IPAddress",
            ],
        )
    } else {
        (
            "sh",
            &[
                "-c",
                "ip -4 -o addr show 2>/dev/null | awk '{print $4}' | cut -d/ -f1",
            ],
        )
    };
    Command::new(prog)
        .args(args)
        .output()
        .ok()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn detect_device_target(device: Option<&str>) -> Result<String> {
    let txt = capture_timeout("adb", &["devices"], Duration::from_secs(5))
        .context("adb devices timed out or failed")?;
    let entries: Vec<(String, String)> = txt
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(id, state)| (id.trim().to_string(), state.trim().to_string()))
        .collect();
    if entries.is_empty() {
        bail!("no adb device — connect device and enable USB debugging");
    }
    let summary = || {
        entries
            .iter()
            .map(|(id, state)| format!("{id}:{state}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    if let Some(d) = device {
        match entries.iter().find(|(id, _)| id == d) {
            Some((_, state)) if state == "device" => {}
            Some((_, state)) => bail!("adb device {d} is {state}; authorise it or reconnect it"),
            None => bail!("adb device {d} not found; connected: {}", summary()),
        }
    }
    let serials: Vec<&str> = entries
        .iter()
        .filter(|(_, state)| state == "device")
        .map(|(id, _)| id.as_str())
        .collect();
    if serials.is_empty() {
        bail!("no authorised adb device; connected: {}", summary());
    }
    if device.is_none() && serials.len() > 1 {
        bail!(
            "multiple adb devices attached: {} — pass one as `cargo xtask android dev <device>`",
            serials.join(", ")
        );
    }
    let abi = adb_capture(
        device,
        &["shell", "getprop", "ro.product.cpu.abi"],
        Duration::from_secs(5),
    )
    .context("adb shell getprop ro.product.cpu.abi timed out or failed")?;
    Ok(match abi.trim() {
        "arm64-v8a" => "aarch64".into(),
        "armeabi-v7a" | "armeabi" => "armv7".into(),
        "x86" => "i686".into(),
        "x86_64" => "x86_64".into(),
        other => bail!("unsupported device ABI: {other}"),
    })
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

fn cmd_cli_push() -> Result<()> {
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

fn cmd_cli_run(args: &[String]) -> Result<()> {
    sh(
        "adb",
        &["shell", &format!("/data/local/tmp/wt {}", args.join(" "))],
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

fn copy_llama_jni(target: &str) -> Result<()> {
    let abi = abi_for(target)?.abi;
    let llama_src = root()
        .join("src-tauri")
        .join("jniLibs")
        .join(abi)
        .join("libllama-cli.so");
    let gen_dir = gen_android()
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
    apply_android_overlay()?;
    let main = gen_android().join("app").join("src").join("main");
    let p = main.join("AndroidManifest.xml");
    if !p.exists() {
        return Ok(());
    }
    let mut raw = fs::read_to_string(&p)?;
    raw = raw.replace("\n        android:extractNativeLibs=\"true\"", "");
    if !raw.contains("android.permission.WAKE_LOCK") {
        let perms = concat!(
            "    <uses-permission android:name=\"android.permission.WAKE_LOCK\" />\n",
            "    <uses-permission android:name=\"android.permission.FOREGROUND_SERVICE\" />\n",
            "    <uses-permission android:name=\"android.permission.FOREGROUND_SERVICE_DATA_SYNC\" />\n",
            "    <uses-permission android:name=\"android.permission.POST_NOTIFICATIONS\" />\n",
            "    <uses-permission android:name=\"android.permission.REQUEST_IGNORE_BATTERY_OPTIMIZATIONS\" />\n",
            "    <uses-feature",
        );
        raw = raw.replacen("    <uses-feature", perms, 1);
    }
    if !raw.contains(".TranscriptionService") {
        let service = concat!(
            "        <service\n",
            "            android:name=\".TranscriptionService\"\n",
            "            android:exported=\"false\"\n",
            "            android:foregroundServiceType=\"dataSync\" />\n\n",
            "        <provider",
        );
        raw = raw.replacen("        <provider", service, 1);
    }
    fs::write(&p, raw)?;
    Ok(())
}

const MAIN_ACTIVITY_KT: &str = include_str!(
    "../../src-tauri/android-overlay/java/com/asolopovas/wtranscriber/MainActivity.kt"
);
const TRANSCRIPTION_SERVICE_KT: &str = include_str!(
    "../../src-tauri/android-overlay/java/com/asolopovas/wtranscriber/TranscriptionService.kt"
);
const STRINGS_XML: &str = include_str!("../../src-tauri/android-overlay/res/values/strings.xml");

fn apply_android_overlay() -> Result<()> {
    let main = gen_android().join("app").join("src").join("main");
    if !main.exists() {
        return Ok(());
    }
    let java_dir = main
        .join("java")
        .join("com")
        .join("asolopovas")
        .join("wtranscriber");
    let res_dir = main.join("res").join("values");
    write_if_changed(&java_dir.join("MainActivity.kt"), MAIN_ACTIVITY_KT)?;
    write_if_changed(
        &java_dir.join("TranscriptionService.kt"),
        TRANSCRIPTION_SERVICE_KT,
    )?;
    write_if_changed(&res_dir.join("strings.xml"), STRINGS_XML)?;
    apply_android_icons(&main.join("res"))?;
    Ok(())
}

fn apply_android_icons(res: &Path) -> Result<()> {
    let icons = root().join("src-tauri").join("icons").join("android");
    if !icons.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(&icons)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let dst_dir = res.join(entry.file_name());
        fs::create_dir_all(&dst_dir)?;
        for file in fs::read_dir(entry.path())? {
            let file = file?;
            if file.file_type()?.is_file() {
                copy_if_changed(&file.path(), &dst_dir.join(file.file_name()))?;
            }
        }
    }
    Ok(())
}

fn write_if_changed(path: &Path, content: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    if fs::read_to_string(path).is_ok_and(|existing| existing == content) {
        return Ok(());
    }
    fs::write(path, content)?;
    Ok(())
}

fn copy_if_changed(src: &Path, dst: &Path) -> Result<()> {
    if let (Ok(a), Ok(b)) = (fs::read(src), fs::read(dst))
        && a == b
    {
        return Ok(());
    }
    fs::copy(src, dst)?;
    Ok(())
}

pub fn sign_patch_inline() -> Result<i32> {
    let gradle = gen_android().join("app").join("build.gradle.kts");
    if !gradle.exists() {
        println!(
            "sign-patch: gen/android not found — run `xtask android prebuilts` + tauri android init first"
        );
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
    let find_line = |start: usize, pred: &dyn Fn(&str) -> bool| -> Option<usize> {
        (start..lines.len()).find(|&i| pred(lines[i].trim_end_matches('\r')))
    };
    let Some(android_idx) = find_line(0, &|l| l.starts_with("android {")) else {
        println!("sign-patch: `android {{` block not found — skipping");
        return Ok(0);
    };
    let load_props: Vec<String> = [
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
    ]
    .into();
    let release_idx = find_line(android_idx, &|l| {
        let t = l.trim();
        t.starts_with("getByName(\"release\")") || t.starts_with("release {")
    });
    let mut new_lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    new_lines.splice((android_idx + 1)..(android_idx + 1), load_props.clone());
    if let Some(rel_idx) = release_idx.map(|i| i + load_props.len())
        && !new_lines[rel_idx].contains("signingConfig")
    {
        new_lines.insert(
            rel_idx + 1,
            "            signingConfig = signingConfigs.getByName(\"release\")".into(),
        );
    }
    let mut joined = new_lines.join("\n");
    if eol == "\r\n" {
        joined = joined.replace('\n', "\r\n");
    }
    fs::write(&gradle, joined)?;
    println!("sign-patch: applied");
    Ok(0)
}
