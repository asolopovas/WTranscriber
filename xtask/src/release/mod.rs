mod artifacts;
mod builders;
mod config;
mod windows_vm;

use anyhow::{Result, bail};
use chrono::Utc;
use clap::Args as ClapArgs;
use std::fs;
use std::path::PathBuf;
use std::thread;

use self::artifacts::{copy_into_channel, find_apk, find_deb, find_host_bundle, write_sha256sums};
pub use self::builders::ensure_dev_keystore_properties;
use self::builders::{build_android, build_deb_docker, build_host};
use self::config::ReleaseConfig;
use self::windows_vm::{build_windows_vm, fetch_windows_vm_exe};
use crate::util::{
    SharedOut, configure_parallel_build_env, git_branch, git_short_sha, is_windows, parallel_jobs,
    pkg_version, root, run_streamed, shared_out,
};

type BuildTask = (&'static str, Box<dyn FnOnce(SharedOut) -> i32 + Send>);

#[derive(ClapArgs)]
#[command(
    about = "Build the full release matrix (Linux .deb + Windows .exe + Android .apk); auto-detects host: Linux builds Windows via the configured VM, Windows builds Linux via Docker (debian:12)"
)]
pub struct Args {
    #[arg(long)]
    pub dev: bool,
    #[arg(long)]
    pub no_host: bool,
    #[arg(long)]
    pub no_android: bool,
    #[arg(long)]
    pub no_deb: bool,
    #[arg(long)]
    pub no_windows_vm: bool,
    #[arg(long)]
    pub skip_rebuild: bool,
    #[arg(long)]
    pub sequential: bool,
}

pub fn run(args: Args) -> Result<()> {
    let ver = pkg_version()?;
    let sha = git_short_sha()?;
    let branch = git_branch()?;
    let out_dir = root().join("releases");
    let out_channel_dir = if args.dev {
        out_dir.join("dev")
    } else {
        out_dir.clone()
    };

    println!(
        "release-build: ver={ver} sha={sha} branch={branch} channel={} parallel={}",
        if args.dev { "dev" } else { "stable" },
        !args.sequential
    );
    let jobs = parallel_jobs();
    configure_parallel_build_env(jobs);
    println!("→ native build jobs: {jobs}");

    if !args.skip_rebuild {
        prewarm()?;
    }

    let release_config = ReleaseConfig::load()?;
    let lock = shared_out();
    let mut tasks: Vec<BuildTask> = Vec::new();
    if !args.no_host {
        let l = lock.clone();
        let skip = args.skip_rebuild;
        tasks.push((
            "host",
            Box::new(move |_| build_host(skip, &l).unwrap_or(127)),
        ));
    }
    if !args.no_android {
        let l = lock.clone();
        let skip = args.skip_rebuild;
        let dev = args.dev;
        tasks.push((
            "and",
            Box::new(move |_| build_android(skip, dev, &l).unwrap_or(127)),
        ));
    }
    let skip = args.skip_rebuild;
    let dev = args.dev;
    match (is_windows(), args.no_deb, args.no_windows_vm) {
        (true, false, _) => {
            let l = lock.clone();
            tasks.push((
                "deb",
                Box::new(move |_| build_deb_docker(skip, &l).unwrap_or(127)),
            ));
        }
        (false, _, false) => {
            let l = lock.clone();
            let cfg = release_config.windows_vm.clone();
            tasks.push((
                "win",
                Box::new(move |_| build_windows_vm(skip, dev, &cfg, &l).unwrap_or(127)),
            ));
        }
        _ => {}
    }

    println!(
        "→ launching {} build(s) {}",
        tasks.len(),
        if args.sequential {
            "sequentially"
        } else {
            "in parallel"
        }
    );

    let mut results: std::collections::HashMap<&'static str, i32> =
        std::collections::HashMap::new();
    if args.sequential {
        for (name, f) in tasks {
            let rc = f(lock.clone());
            results.insert(name, rc);
        }
    } else {
        let handles: Vec<_> = tasks
            .into_iter()
            .map(|(name, f)| {
                let l = lock.clone();
                thread::spawn(move || {
                    let rc = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(l)))
                        .unwrap_or(101);
                    (name, rc)
                })
            })
            .collect();
        for h in handles {
            let (name, rc) = h.join().unwrap_or(("unknown", 101));
            results.insert(name, rc);
        }
    }

    let mut artifacts: Vec<PathBuf> = Vec::new();

    if !args.no_host {
        let rc = *results.get("host").unwrap_or(&-1);
        if rc != 0 {
            bail!("host build failed (exit {rc})");
        }
        if let Some((src, name)) = find_host_bundle(&ver, &branch, args.dev) {
            artifacts.push(copy_into_channel(&src, &name, &out_channel_dir)?);
        }
    }

    if let Some(&rc) = results.get("deb") {
        match rc {
            0 => {
                if let Some((src, name)) = find_deb(&ver, &branch, args.dev) {
                    artifacts.push(copy_into_channel(&src, &name, &out_channel_dir)?);
                }
            }
            _ => eprintln!("⚠  docker .deb build failed (exit {rc}); continuing without .deb"),
        }
    }

    if let Some(&rc) = results.get("win") {
        match rc {
            -1 => eprintln!("⚠  windows-vm SSH unreachable — skipping Windows build"),
            0 => match fetch_windows_vm_exe(
                &release_config.windows_vm,
                &ver,
                &branch,
                args.dev,
                &out_channel_dir,
            ) {
                Ok(Some(p)) => artifacts.push(p),
                Ok(None) => eprintln!("⚠  windows-vm produced no -setup.exe"),
                Err(e) => eprintln!("⚠  windows-vm scp failed: {e:#}"),
            },
            _ => eprintln!("⚠  windows-vm build failed (exit {rc}); continuing without .exe"),
        }
    }

    if !args.no_android {
        let rc = *results.get("and").unwrap_or(&-1);
        if rc != 0 {
            bail!("android build failed (exit {rc})");
        }
        match find_apk(args.dev)? {
            Some(apk) => {
                if !apk.signed && !args.dev {
                    bail!(
                        "refusing to publish unsigned APK on stable channel. Configure src-tauri/gen/android/keystore.properties."
                    );
                }
                if !apk.signed {
                    eprintln!(
                        "⚠  APK is UNSIGNED — Android will refuse to install. Configure keystore.properties for distributable builds."
                    );
                }
                let dst = if args.dev {
                    format!("wtranscriber-{branch}.apk")
                } else {
                    format!("wtranscriber-{ver}.apk")
                };
                artifacts.push(copy_into_channel(&apk.path, &dst, &out_channel_dir)?);
            }
            None => eprintln!("⚠  no APK produced"),
        }
    }

    if artifacts.is_empty() {
        bail!("no artifacts produced");
    }

    fs::create_dir_all(&out_channel_dir)?;

    let sums_path = if args.dev {
        out_channel_dir.join("SHA256SUMS")
    } else {
        out_dir.join(format!("SHA256SUMS-{ver}"))
    };
    write_sha256sums(&artifacts, &sums_path)?;
    artifacts.push(sums_path.clone());
    println!("  + {}", sums_path.display());

    if let Ok(_key) = std::env::var("TAURI_SIGNING_PRIVATE_KEY") {
        println!("→ TAURI_SIGNING_PRIVATE_KEY set — generating updater signatures (.sig)");
        let mut new_sigs = Vec::new();
        for p in artifacts.iter() {
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !["exe", "AppImage", "deb", "apk"].contains(&ext) {
                continue;
            }
            let lock_clone = shared_out();
            let rc = run_streamed(
                "sig",
                "bun",
                &[
                    "run",
                    "tauri",
                    "signer",
                    "sign",
                    "--private-key",
                    &std::env::var("TAURI_SIGNING_PRIVATE_KEY").unwrap_or_default(),
                    p.to_string_lossy().as_ref(),
                ],
                &[],
                &lock_clone,
            )?;
            if rc != 0 {
                bail!("signer failed for {} (exit {rc})", p.display());
            }
            let sig = p.with_extension(format!("{ext}.sig"));
            if !sig.exists() {
                bail!("signer produced no signature for {}", p.display());
            }
            new_sigs.push(sig);
        }
        artifacts.extend(new_sigs);
    }

    let manifest_path = if args.dev {
        out_channel_dir.join("release-manifest.json")
    } else {
        out_dir.join(format!("release-manifest-{ver}.json"))
    };
    let manifest = serde_json::json!({
        "channel": if args.dev { "dev" } else { "stable" },
        "version": ver,
        "branch": branch,
        "sha": sha,
        "builtAt": Utc::now().to_rfc3339(),
        "artifacts": artifacts.iter().map(|p| p.file_name().unwrap().to_string_lossy()).collect::<Vec<_>>(),
    });
    fs::write(&manifest_path, format!("{manifest:#}\n"))?;
    artifacts.push(manifest_path.clone());
    println!("  + {}", manifest_path.display());

    let list_name = if args.dev {
        ".release-dev-artifacts"
    } else {
        ".release-stable-artifacts"
    };
    fs::write(
        out_dir.join(list_name),
        artifacts
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n",
    )?;
    println!("✓ release-build done ({} files)", artifacts.len());
    Ok(())
}

fn prewarm() -> Result<()> {
    println!("→ pre-warm: bun install");
    let lock = shared_out();
    let rc = run_streamed("bun", "bun", &["install", "--no-progress"], &[], &lock)?;
    if rc != 0 {
        bail!("bun install failed (exit {rc})");
    }
    println!("→ pre-warm: bun run build (frontend → dist/)");
    let rc = run_streamed("front", "bun", &["run", "build"], &[], &lock)?;
    if rc != 0 {
        bail!("frontend build failed (exit {rc})");
    }
    println!("→ pre-warm: cargo fetch");
    let rc = run_streamed(
        "fetch",
        "cargo",
        &["fetch", "--manifest-path", "src-tauri/Cargo.toml"],
        &[],
        &lock,
    )?;
    if rc != 0 {
        bail!("cargo fetch failed (exit {rc})");
    }
    patch_whisper_rs_sys()?;
    Ok(())
}

/// Patch whisper-rs-sys 0.15.0 in the cargo registry so its build.rs gates
/// `/utf-8` on the *target* being Windows MSVC, not the *host*. Upstream
/// hardcodes `cfg!(target_os = "windows")`, which injects `/utf-8` into
/// CXXFLAGS for any build run on a Windows host — breaking the Android
/// cross-build because NDK clang treats `/utf-8` as a path.
///
/// Idempotent: detects already-patched files and skips.
fn patch_whisper_rs_sys() -> Result<()> {
    let home = dirs_home();
    let Some(home) = home else { return Ok(()) };
    let src_root = home.join(".cargo").join("registry").join("src");
    let Ok(entries) = fs::read_dir(&src_root) else {
        return Ok(());
    };
    let needle = "if cfg!(target_os = \"windows\") {\n        config.cxxflag(\"/utf-8\");";
    let patched_marker = "// wtranscriber-patched: target-aware /utf-8";
    let replacement = "{\n        let target = std::env::var(\"TARGET\").unwrap_or_default();\n        if target.contains(\"windows\") && target.contains(\"msvc\") {\n            config.cxxflag(\"/utf-8\");\n        }\n        if target.contains(\"windows\") {\n            println!(\"cargo:rustc-link-lib=advapi32\");\n        }\n    } // wtranscriber-patched: target-aware /utf-8\n    if false {\n        config.cxxflag(\"/utf-8\");";
    let mut patched_any = false;
    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_dir() {
            continue;
        }
        let build_rs = p.join("whisper-rs-sys-0.15.0").join("build.rs");
        if !build_rs.exists() {
            continue;
        }
        let body = fs::read_to_string(&build_rs)?;
        if body.contains(patched_marker) {
            continue;
        }
        let Some(idx) = body.find(needle) else {
            continue;
        };
        let mut new_body = String::with_capacity(body.len() + replacement.len());
        new_body.push_str(&body[..idx]);
        new_body.push_str("if cfg!(target_os = \"windows\") ");
        new_body.push_str(replacement);
        new_body.push_str(&body[idx + needle.len()..]);
        fs::write(&build_rs, new_body)?;
        println!(
            "→ patched {} (gating /utf-8 on target, not host)",
            build_rs.display()
        );
        patched_any = true;
    }
    if !patched_any {
        // Either already patched on every checkout, or the crate isn't in
        // registry yet (cargo fetch didn't extract). Either way: not fatal.
    }
    Ok(())
}

fn dirs_home() -> Option<PathBuf> {
    if let Ok(v) = std::env::var("HOME") {
        if !v.is_empty() {
            return Some(PathBuf::from(v));
        }
    }
    if let Ok(v) = std::env::var("USERPROFILE") {
        if !v.is_empty() {
            return Some(PathBuf::from(v));
        }
    }
    None
}
