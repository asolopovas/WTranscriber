use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use clap::{Args as ClapArgs, Subcommand};
use sha2::{Digest, Sha256};

use crate::util::{SharedOut, exe, root, run_streamed, sh, shared_out};

const ARCHES: [&str; 5] = ["61", "75", "80", "86", "89"];
const DEFAULT_TAG: &str = "cuda";

#[derive(Subcommand)]
#[command(about = "Build and publish optional Windows Whisper CUDA worker zips")]
pub enum Cmd {
    Build(BuildArgs),
    Publish(PublishArgs),
    Release(BuildArgs),
}

#[derive(ClapArgs, Clone)]
pub struct BuildArgs {
    #[arg(long, value_delimiter = ',', help = "CUDA SM arch list, e.g. 61,86")]
    pub arch: Vec<String>,
    #[arg(long, help = "Output directory; defaults to releases/cuda")]
    pub out_dir: Option<PathBuf>,
}

#[derive(ClapArgs)]
pub struct PublishArgs {
    #[arg(long, help = "GitHub release tag; defaults to cuda")]
    pub tag: Option<String>,
    #[arg(long, help = "Artifact directory; defaults to releases/<tag>")]
    pub dir: Option<PathBuf>,
}

pub fn run(cmd: Cmd) -> Result<()> {
    match cmd {
        Cmd::Build(args) => build(args).map(|_| ()),
        Cmd::Publish(args) => publish(args),
        Cmd::Release(args) => {
            let tag = default_tag();
            let out_dir = build(args)?;
            publish(PublishArgs {
                tag: Some(tag),
                dir: Some(out_dir),
            })
        }
    }
}

fn default_tag() -> String {
    DEFAULT_TAG.to_string()
}

fn selected_arches(args: &BuildArgs) -> Result<Vec<String>> {
    if args.arch.is_empty() {
        return Ok(ARCHES.iter().map(ToString::to_string).collect());
    }
    let mut out = Vec::new();
    for arch in &args.arch {
        if !ARCHES.contains(&arch.as_str()) {
            bail!("unsupported CUDA worker arch `{arch}`; expected one of {ARCHES:?}");
        }
        if !out.contains(arch) {
            out.push(arch.clone());
        }
    }
    Ok(out)
}

fn default_out_dir(tag: &str) -> PathBuf {
    root().join("releases").join(tag)
}

fn build(args: BuildArgs) -> Result<PathBuf> {
    if !cfg!(target_os = "windows") {
        bail!("Windows CUDA workers must be built on Windows");
    }
    let tag = default_tag();
    let out_dir = args
        .out_dir
        .clone()
        .unwrap_or_else(|| default_out_dir(&tag));
    fs::create_dir_all(&out_dir)?;
    let lock = shared_out();
    for arch in selected_arches(&args)? {
        package_worker(&arch, &out_dir, &lock)?;
    }
    write_sha256sums(&zip_artifacts(&out_dir)?, &out_dir.join("SHA256SUMS"))?;
    println!("✓ CUDA workers written to {}", out_dir.display());
    Ok(out_dir)
}

fn build_jobs() -> String {
    thread::available_parallelism()
        .map_or(4, std::num::NonZero::get)
        .to_string()
}

fn cargo_env(arch: &str, jobs: &str) -> Vec<(&'static str, String)> {
    unsafe { std::env::remove_var("CARGO_INCREMENTAL") };
    let mut env = vec![
        ("CARGO_INCREMENTAL", "0".into()),
        ("CMAKE_CUDA_ARCHITECTURES", arch.into()),
        ("CMAKE_BUILD_PARALLEL_LEVEL", jobs.into()),
        ("CMAKE_C_COMPILER_LAUNCHER", String::new()),
        ("CMAKE_CXX_COMPILER_LAUNCHER", String::new()),
        ("RUSTC_WRAPPER", String::new()),
        ("SCCACHE_DISABLE", "1".into()),
        ("CL", "/FS".into()),
    ];
    if let Some(path) = path_without_ccache() {
        env.push(("PATH", path));
    }
    env
}

// `cargo clean -p` does not reliably remove build-script out dirs, and a
// CMakeCache from a previous configure pins GGML_CCACHE_FOUND and the CUDA
// arch list. Remove the dirs so every arch configures fresh.
fn remove_whisper_sys_build_dirs() {
    let build_dir = root()
        .join("workers")
        .join("whisper-cuda-worker")
        .join("target")
        .join("release")
        .join("build");
    let Ok(entries) = fs::read_dir(&build_dir) else {
        return;
    };
    for entry in entries.flatten() {
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with("whisper-rs-sys-")
        {
            fs::remove_dir_all(entry.path()).ok();
        }
    }
}

// ggml's CMake auto-detects ccache on PATH when no compiler launcher is set;
// ccache + MSVC emits no object files and the build dies at lib.exe.
fn path_without_ccache() -> Option<String> {
    let path = std::env::var_os("PATH")?;
    let dirs: Vec<_> = std::env::split_paths(&path).collect();
    let kept: Vec<_> = dirs
        .iter()
        .filter(|dir| !dir.join("ccache.exe").exists() && !dir.join("ccache").exists())
        .cloned()
        .collect();
    if kept.len() == dirs.len() {
        return None;
    }
    Some(
        std::env::join_paths(kept)
            .ok()?
            .to_string_lossy()
            .into_owned(),
    )
}

fn run_streamed_owned_heartbeat(
    tag: &str,
    prog: &str,
    args: &[&str],
    env: &[(&str, String)],
    lock: &SharedOut,
) -> Result<i32> {
    let prefix = format!("[{tag}] ");
    let mut cmd = Command::new(prog);
    cmd.args(args)
        .current_dir(root())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().with_context(|| format!("spawn {prog}"))?;
    let stdout = child.stdout.take().context("no stdout")?;
    let stderr = child.stderr.take().context("no stderr")?;
    let done = Arc::new(AtomicBool::new(false));
    let prefix_o = prefix.clone();
    let lock_o = lock.clone();
    let h_out = thread::spawn(move || forward_lines(stdout, &prefix_o, &lock_o));
    let prefix_e = prefix.clone();
    let lock_e = lock.clone();
    let h_err = thread::spawn(move || forward_lines(stderr, &prefix_e, &lock_e));
    let done_h = done.clone();
    let prefix_h = prefix.clone();
    let lock_h = lock.clone();
    let started = Instant::now();
    let h_heartbeat = thread::spawn(move || {
        while !done_h.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_secs(30));
            if done_h.load(Ordering::Relaxed) {
                break;
            }
            let processes = cuda_build_process_summary();
            let _g = lock_h.lock().unwrap();
            let mut stdout = std::io::stdout().lock();
            let _ = writeln!(
                stdout,
                "{prefix_h}still compiling CUDA worker ({:.0}s elapsed; {processes})",
                started.elapsed().as_secs_f64()
            );
        }
    });
    let status = child.wait()?;
    done.store(true, Ordering::Relaxed);
    let _ = h_out.join();
    let _ = h_err.join();
    let _ = h_heartbeat.join();
    Ok(status.code().unwrap_or(1))
}

fn forward_lines<R: std::io::Read>(reader: R, prefix: &str, lock: &SharedOut) {
    let r = BufReader::new(reader);
    for line in r.lines().map_while(|l| l.ok()) {
        let _g = lock.lock().unwrap();
        let mut stdout = std::io::stdout().lock();
        let _ = writeln!(stdout, "{prefix}{line}");
    }
}

fn cuda_build_process_summary() -> String {
    if !cfg!(target_os = "windows") {
        return "waiting for cargo output".into();
    }
    let names = ["nvcc.exe", "cl.exe", "cmake.exe", "ninja.exe"];
    let mut parts = Vec::new();
    for name in names {
        if let Some(count) = task_count(name)
            && count > 0
        {
            parts.push(format!("{name}={count}"));
        }
    }
    if parts.is_empty() {
        "waiting for cargo output".into()
    } else {
        parts.join(", ")
    }
}

fn task_count(name: &str) -> Option<usize> {
    let out = Command::new("tasklist")
        .args(["/FI", &format!("IMAGENAME eq {name}"), "/NH"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let needle = name.to_ascii_lowercase();
    let count = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|line| line.to_ascii_lowercase().contains(&needle))
        .count();
    Some(count)
}

fn package_worker(arch: &str, out_dir: &Path, lock: &SharedOut) -> Result<()> {
    println!("→ building Whisper CUDA worker sm_{arch}");
    let clean_rc = run_streamed(
        "cuda-clean",
        "cargo",
        &[
            "clean",
            "--manifest-path",
            "workers/whisper-cuda-worker/Cargo.toml",
            "-p",
            "whisper-rs-sys",
        ],
        &[],
        lock,
    )?;
    if clean_rc != 0 {
        bail!("cargo clean failed for CUDA worker sm_{arch} (exit {clean_rc})");
    }
    remove_whisper_sys_build_dirs();
    let jobs = build_jobs();
    println!("→ CUDA worker sm_{arch}: using {jobs} parallel build jobs");
    let env = cargo_env(arch, &jobs);
    let rc = run_streamed_owned_heartbeat(
        "cuda-worker",
        "cargo",
        &[
            "build",
            "--manifest-path",
            "workers/whisper-cuda-worker/Cargo.toml",
            "--release",
            "-j",
            &jobs,
            "--features",
            "cuda",
        ],
        &env,
        lock,
    )?;
    if rc != 0 {
        bail!("CUDA worker sm_{arch} build failed (exit {rc})");
    }

    let stage_root = out_dir.join("stage");
    let stage = stage_root.join(format!("sm{arch}"));
    fs::remove_dir_all(&stage).ok();
    fs::create_dir_all(&stage)?;
    let worker = root()
        .join("workers")
        .join("whisper-cuda-worker")
        .join("target")
        .join("release")
        .join(exe("wt-whisper-cuda-worker"));
    fs::copy(&worker, stage.join("wt-whisper-cuda-worker.exe"))
        .with_context(|| format!("copy {}", worker.display()))?;
    fs::write(stage.join("arch.txt"), format!("sm_{arch}\n"))?;

    let zip = out_dir.join(format!("wtranscriber-cuda-sm{arch}-win-x64.zip"));
    fs::remove_file(&zip).ok();
    let stage_glob = stage.join("*").to_string_lossy().replace('\'', "''");
    let zip_path = zip.to_string_lossy().replace('\'', "''");
    let script =
        format!("Compress-Archive -Force -Path '{stage_glob}' -DestinationPath '{zip_path}'");
    let zip_rc = run_streamed(
        "cuda-zip",
        "powershell.exe",
        &["-NoLogo", "-NoProfile", "-Command", &script],
        &[],
        lock,
    )?;
    if zip_rc != 0 {
        bail!("zip failed for CUDA worker sm_{arch} (exit {zip_rc})");
    }
    fs::remove_dir_all(&stage_root).ok();
    println!("  + {}", zip.display());
    Ok(())
}

fn zip_artifacts(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut zips: Vec<PathBuf> = fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("zip"))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("wtranscriber-cuda-sm"))
        })
        .collect();
    zips.sort();
    if zips.is_empty() {
        bail!("no CUDA worker zips found in {}", dir.display());
    }
    Ok(zips)
}

fn write_sha256sums(artifacts: &[PathBuf], sums_path: &Path) -> Result<()> {
    let mut lines = Vec::new();
    for p in artifacts {
        let bytes = fs::read(p)?;
        let mut h = Sha256::new();
        h.update(&bytes);
        let digest = h.finalize();
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        let name = p.file_name().context("no filename")?.to_string_lossy();
        lines.push(format!("{hex}  {name}"));
    }
    fs::write(sums_path, format!("{}\n", lines.join("\n")))?;
    println!("  + {}", sums_path.display());
    Ok(())
}

fn publish(args: PublishArgs) -> Result<()> {
    let tag = args.tag.unwrap_or_else(default_tag);
    let dir = args.dir.unwrap_or_else(|| default_out_dir(&tag));
    let mut artifacts = zip_artifacts(&dir)?;
    let sums = dir.join("SHA256SUMS");
    if !sums.exists() {
        write_sha256sums(&artifacts, &sums)?;
    }
    artifacts.push(sums);
    ensure_gh_config_dir();
    if !release_exists(&tag) {
        sh(
            "gh",
            &[
                "release",
                "create",
                &tag,
                "--title",
                &tag,
                "--notes",
                "Optional WTranscriber Whisper CUDA worker binaries. The main installer downloads the matching sm-specific zip on NVIDIA systems.",
                "--latest=false",
            ],
        )?;
    }
    upload_with_retry(&tag, &artifacts)?;
    println!("✓ CUDA workers: https://github.com/asolopovas/WTranscriber/releases/tag/{tag}");
    Ok(())
}

fn upload_with_retry(tag: &str, artifacts: &[PathBuf]) -> Result<()> {
    for path in artifacts {
        let path_arg = path.to_string_lossy().to_string();
        for attempt in 1..=3u32 {
            let status = Command::new("gh")
                .args(["release", "upload", tag, &path_arg, "--clobber"])
                .current_dir(root())
                .status()
                .with_context(|| format!("spawn gh release upload for {}", path.display()))?;
            if status.success() {
                break;
            }
            if attempt == 3 {
                bail!(
                    "gh release upload failed for {} after 3 attempts",
                    path.display()
                );
            }
            eprintln!(
                "upload of {} failed on attempt {attempt}; retrying in 5s",
                path.display()
            );
            thread::sleep(Duration::from_secs(5));
        }
    }
    Ok(())
}

fn release_exists(tag: &str) -> bool {
    std::process::Command::new("gh")
        .args(["release", "view", tag])
        .current_dir(root())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn ensure_gh_config_dir() {
    if std::env::var_os("GH_CONFIG_DIR").is_some() {
        return;
    }
    if cfg!(windows)
        && let Some(profile) = std::env::var_os("USERPROFILE")
    {
        let p = PathBuf::from(profile)
            .join("AppData")
            .join("Roaming")
            .join("GitHub CLI");
        if p.join("hosts.yml").exists() {
            unsafe { std::env::set_var("GH_CONFIG_DIR", &p) };
        }
    }
}
