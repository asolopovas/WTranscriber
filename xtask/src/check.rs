use anyhow::{Context, Result, bail};
use clap::Args as ClapArgs;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;

use crate::util::{SharedOut, root, run_streamed, shared_out};

#[derive(ClapArgs)]
#[command(about = "Run the local quality gate")]
pub struct Args {
    #[arg(long)]
    pub sequential: bool,
    #[arg(value_name = "JOB")]
    pub jobs: Vec<String>,
}

#[derive(Clone, Copy)]
struct Job {
    tag: &'static str,
    command: &'static str,
}

pub fn run(args: Args) -> Result<()> {
    let jobs = select_jobs(args.jobs)?;
    ensure_tools(&jobs)?;
    if jobs
        .iter()
        .any(|job| matches!(job.tag, "clippy" | "rust-test"))
    {
        invalidate_stale_cmake_caches()?;
    }

    let out = shared_out();
    let mut results = Vec::with_capacity(jobs.len());

    if args.sequential {
        for job in jobs {
            let code = run_job(&job, &out)?;
            results.push((job.tag, code));
        }
    } else {
        let handles: Vec<_> = jobs
            .into_iter()
            .map(|job| {
                let out = out.clone();
                thread::spawn(move || {
                    let code = run_job(&job, &out).unwrap_or(127);
                    (job.tag, code)
                })
            })
            .collect();
        for handle in handles {
            results.push(handle.join().unwrap_or(("unknown", 101)));
        }
    }

    if let Some((tag, code)) = results.iter().find(|(_, code)| *code != 0) {
        bail!("check job {tag} failed with exit {code}");
    }
    println!("✓ check passed ({} jobs)", results.len());
    Ok(())
}

fn select_jobs(selected: Vec<String>) -> Result<Vec<Job>> {
    let jobs = jobs();
    if selected.is_empty() {
        return Ok(jobs);
    }
    let mut out = Vec::with_capacity(selected.len());
    for tag in selected {
        let Some(index) = jobs.iter().position(|job| job.tag == tag) else {
            let known = jobs
                .iter()
                .map(|job| job.tag)
                .collect::<Vec<_>>()
                .join(", ");
            bail!("unknown check job {tag:?}; known jobs: {known}");
        };
        out.push(jobs[index]);
    }
    Ok(out)
}

fn jobs() -> Vec<Job> {
    vec![
        Job {
            tag: "fmt-check",
            command: "cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check && cargo fmt --manifest-path xtask/Cargo.toml --all -- --check && bun x prettier --check src/**/*.{ts,vue} scripts/**/*.ts *.{json,html,md} docs/**/*.md",
        },
        Job {
            tag: "clippy",
            command: "cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --no-default-features --features sherpa-shared --offline -- -D warnings",
        },
        Job {
            tag: "clippy-xtask",
            command: "cargo clippy --manifest-path xtask/Cargo.toml --target-dir tmp/xtask-check-target --all-targets --offline -- -D warnings",
        },
        Job {
            tag: "typecheck",
            command: "bun run typecheck",
        },
        Job {
            tag: "vue-lint",
            command: "bun run scripts/lint-vue.ts",
        },
        Job {
            tag: "knip",
            command: "bun x knip",
        },
        Job {
            tag: "rust-test",
            command: "cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --features sherpa-shared --offline",
        },
        Job {
            tag: "xtask-test",
            command: "cargo test --manifest-path xtask/Cargo.toml --target-dir tmp/xtask-check-target --offline",
        },
        Job {
            tag: "js-test",
            command: "bun run test",
        },
        Job {
            tag: "machete",
            command: "cargo-machete src-tauri && cargo-machete xtask",
        },
        Job {
            tag: "audit",
            command: "cargo audit --file src-tauri/Cargo.lock && bun audit",
        },
    ]
}

fn run_job(job: &Job, out: &SharedOut) -> Result<i32> {
    if cfg!(windows) {
        run_streamed(job.tag, "cmd", &["/C", job.command], &[], out)
    } else {
        run_streamed(job.tag, "sh", &["-c", job.command], &[], out)
    }
}

fn ensure_tools(jobs: &[Job]) -> Result<()> {
    if jobs.iter().any(|job| job.tag == "machete") {
        ensure_cargo_tool("cargo-machete")?;
    }
    if jobs.iter().any(|job| job.tag == "audit") {
        ensure_cargo_tool("cargo-audit")?;
    }
    Ok(())
}

fn ensure_cargo_tool(tool: &str) -> Result<()> {
    let installed = Command::new(tool)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if installed {
        return Ok(());
    }
    println!("→ installing {tool}");
    let out = shared_out();
    let code = run_streamed(
        "install",
        "cargo",
        &["install", "--locked", tool],
        &[],
        &out,
    )?;
    if code != 0 {
        bail!("cargo install {tool} failed with exit {code}");
    }
    Ok(())
}

fn invalidate_stale_cmake_caches() -> Result<()> {
    let target = root().join("src-tauri").join("target");
    let sentinel = target.join(".cmake-generator");
    let desired = std::env::var("CMAKE_GENERATOR").unwrap_or_default();
    let previous = fs::read_to_string(&sentinel).unwrap_or_default();
    if previous == desired {
        return Ok(());
    }
    for profile in ["debug", "release"] {
        let build = target.join(profile).join("build");
        remove_sys_build_dirs(&build)?;
    }
    fs::create_dir_all(&target).with_context(|| format!("create {}", target.display()))?;
    fs::write(&sentinel, &desired).with_context(|| format!("write {}", sentinel.display()))?;
    if !previous.is_empty() {
        eprintln!(
            "[check] CMAKE_GENERATOR changed ({previous:?} -> {desired:?}); wiped whisper-rs-sys / sherpa-onnx-sys build dirs"
        );
    }
    Ok(())
}

fn remove_sys_build_dirs(build: &Path) -> Result<()> {
    let Ok(entries) = fs::read_dir(build) else {
        return Ok(());
    };
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("whisper-rs-sys-") || name.starts_with("sherpa-onnx-sys-") {
            fs::remove_dir_all(entry.path())
                .with_context(|| format!("remove {}", entry.path().display()))?;
        }
    }
    Ok(())
}
