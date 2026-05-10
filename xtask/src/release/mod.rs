mod artifacts;
mod builders;
mod windows_vm;

use anyhow::{Result, bail};
use chrono::Utc;
use clap::Args as ClapArgs;
use std::fs;
use std::path::PathBuf;
use std::thread;

use self::artifacts::{
    copy_into_channel, find_apk, find_host_bundle, find_wsl_deb, write_sha256sums,
};
use self::builders::{build_android, build_host, build_wsl};
use self::windows_vm::{build_windows_vm, fetch_windows_vm_exe};
use crate::util::{
    SharedOut, git_branch, git_short_sha, is_windows, pkg_version, root, run_streamed, shared_out,
};

type BuildTask = (&'static str, Box<dyn FnOnce(SharedOut) -> i32 + Send>);

#[derive(ClapArgs)]
#[command(about = "Build release artifacts (host + Android + WSL deb on Windows)")]
pub struct Args {
    #[arg(long)]
    pub dev: bool,
    #[arg(long)]
    pub no_host: bool,
    #[arg(long)]
    pub no_android: bool,
    #[arg(long)]
    pub no_wsl: bool,
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

    if !args.skip_rebuild {
        prewarm()?;
    }

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
    if !args.no_wsl && is_windows() {
        let l = lock.clone();
        let skip = args.skip_rebuild;
        tasks.push(("wsl", Box::new(move |_| build_wsl(skip, &l).unwrap_or(127))));
    }
    if !is_windows() && !args.no_windows_vm {
        let l = lock.clone();
        let skip = args.skip_rebuild;
        let dev = args.dev;
        tasks.push((
            "win",
            Box::new(move |_| build_windows_vm(skip, dev, &l).unwrap_or(127)),
        ));
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
                thread::spawn(move || (name, f(l)))
            })
            .collect();
        for h in handles {
            let (name, rc) = h.join().expect("thread panicked");
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

    if !args.no_wsl
        && is_windows()
        && let Some(&rc) = results.get("wsl")
    {
        if rc == -1 {
            eprintln!("⚠  WSL build skipped (no distro with bun + cargo)");
        } else if rc != 0 {
            eprintln!("⚠  WSL build failed (exit {rc}); continuing without .deb");
        } else if let Some((src, name)) = find_wsl_deb(&ver, &branch, args.dev) {
            artifacts.push(copy_into_channel(&src, &name, &out_channel_dir)?);
        }
    }

    if !is_windows()
        && !args.no_windows_vm
        && let Some(&rc) = results.get("win")
    {
        if rc == -1 {
            eprintln!("⚠  windows-vm SSH unreachable — skipping Windows build");
        } else if rc != 0 {
            eprintln!("⚠  windows-vm build failed (exit {rc}); continuing without .exe");
        } else {
            match fetch_windows_vm_exe(&ver, &branch, args.dev, &out_channel_dir) {
                Ok(Some(p)) => artifacts.push(p),
                Ok(None) => eprintln!("⚠  windows-vm produced no -setup.exe"),
                Err(e) => eprintln!("⚠  windows-vm scp failed: {e:#}"),
            }
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
            let _ = run_streamed(
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
            );
            let sig = p.with_extension(format!("{ext}.sig"));
            if sig.exists() {
                new_sigs.push(sig);
            }
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
    println!("→ pre-warm: cargo fetch");
    let lock = shared_out();
    let _ = run_streamed(
        "fetch",
        "cargo",
        &["fetch", "--manifest-path", "src-tauri/Cargo.toml"],
        &[],
        &lock,
    );
    Ok(())
}
