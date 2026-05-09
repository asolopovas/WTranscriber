use anyhow::{Context, Result, bail};
use chrono::Utc;
use clap::Args as ClapArgs;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;

use crate::util::{
    SharedOut, exe, git_branch, git_short_sha, is_windows, pkg_version, root, run_streamed,
    run_streamed_stdin, shared_out,
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

fn build_host(skip: bool, lock: &SharedOut) -> Result<i32> {
    if skip {
        println!("[host] --skip-rebuild, reusing existing bundle");
        return Ok(0);
    }
    unsafe {
        std::env::remove_var("SHERPA_ONNX_LIB_DIR");
        std::env::remove_var("SHERPA_ONNX_LIB");
        std::env::remove_var("SHERPA_ONNX_INCLUDE_DIR");
    }
    run_streamed(
        "host",
        "bun",
        &[
            "run",
            "tauri",
            "build",
            "--",
            "--no-default-features",
            "--features",
            "sherpa-static",
        ],
        &[("CARGO_INCREMENTAL", "1")],
        lock,
    )
}

fn build_android(skip: bool, dev: bool, lock: &SharedOut) -> Result<i32> {
    if skip {
        println!("[and] --skip-rebuild, reusing existing apk");
        return Ok(0);
    }
    let rc = crate::android::sign_patch_inline()?;
    if rc != 0 {
        return Ok(rc);
    }
    let mut env_vars: Vec<(&str, &str)> = vec![("CARGO_INCREMENTAL", "1")];
    if dev {
        env_vars.push(("WT_DEV_APK", "1"));
    }
    run_streamed(
        "and",
        std::env::current_exe()?.to_string_lossy().as_ref(),
        &["android", "build", "--target", "aarch64"],
        &env_vars,
        lock,
    )
}

fn ssh_alive() -> bool {
    std::process::Command::new("ssh")
        .args([
            "-o",
            "ConnectTimeout=5",
            "-o",
            "BatchMode=yes",
            "windows-vm",
            "true",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn build_windows_vm(skip: bool, _dev: bool, lock: &SharedOut) -> Result<i32> {
    if skip {
        println!("[win] --skip-rebuild, leaving existing artefact alone");
        return Ok(0);
    }
    if !ssh_alive() {
        println!(
            "[win] windows-vm SSH unreachable on localhost:2222 — skipping Windows build.\n\
             [win]   bring up: cd ~/os/windows-vm && make up   (then run shared/enable-ssh.ps1 inside VM once)"
        );
        return Ok(-1);
    }
    let sha = git_short_sha()?;
    let push = run_streamed(
        "win",
        "git",
        &["push", "origin", "HEAD"],
        &[],
        lock,
    )?;
    if push != 0 {
        eprintln!("[win] git push origin HEAD failed (exit {push}) — VM cannot fetch latest commit");
        return Ok(push);
    }
    let script = format!(
        "set -e\n\
         export PATH=\"$HOME/.cargo/bin:$HOME/.bun/bin:/c/Program Files/just:/c/Program Files/nodejs:/c/Program Files/Git/cmd:$PATH\"\n\
         if ! command -v just >/dev/null 2>&1; then\n\
             echo '[win] just missing in VM — run scripts/bootstrap-windows.ps1 inside the VM first' >&2\n\
             exit 91\n\
         fi\n\
         cd /c\n\
         if [ ! -d WTranscriber ]; then git clone https://github.com/asolopovas/WTranscriber.git WTranscriber; fi\n\
         cd /c/WTranscriber\n\
         git fetch --prune --force --tags origin\n\
         git reset --hard {sha}\n\
         git clean -fdx src-tauri/target/release/bundle/nsis 2>/dev/null || true\n\
         just bootstrap\n\
         bun install --frozen-lockfile --no-progress 2>&1 | tail -5\n\
         just build-cpu\n\
         ls src-tauri/target/release/bundle/nsis/*-setup.exe\n"
    );
    run_streamed_stdin(
        "win",
        "ssh",
        &["windows-vm", "bash", "-l"],
        &script,
        &[],
        lock,
    )
}

fn fetch_windows_vm_exe(
    ver: &str,
    branch: &str,
    dev: bool,
    out_channel_dir: &Path,
) -> Result<Option<PathBuf>> {
    let probe = std::process::Command::new("ssh")
        .args([
            "windows-vm",
            "bash",
            "-lc",
            "ls /c/WTranscriber/src-tauri/target/release/bundle/nsis/*-setup.exe 2>/dev/null | head -1",
        ])
        .output()?;
    let remote_path = String::from_utf8_lossy(&probe.stdout).trim().to_string();
    if remote_path.is_empty() {
        return Ok(None);
    }
    fs::create_dir_all(out_channel_dir)?;
    let dst_name = if dev {
        format!("wtranscriber-setup-{branch}.exe")
    } else {
        format!("wtranscriber-setup-{ver}.exe")
    };
    let dst = out_channel_dir.join(&dst_name);
    let scp_src = if let Some(stripped) = remote_path.strip_prefix("/c/") {
        format!("windows-vm:/C:/{stripped}")
    } else {
        format!("windows-vm:{remote_path}")
    };
    let st = std::process::Command::new("scp")
        .args([&scp_src, dst.to_string_lossy().as_ref()])
        .status()?;
    if !st.success() {
        bail!("scp from windows-vm failed (exit {:?})", st.code());
    }
    let size = fs::metadata(&dst)?.len() as f64 / 1024.0 / 1024.0;
    println!("  + {} ({:.1} MB) (windows-vm)", dst.display(), size);
    Ok(Some(dst))
}

fn build_wsl(skip: bool, lock: &SharedOut) -> Result<i32> {
    if skip {
        println!("[wsl] --skip-rebuild, looking for existing .deb");
        return Ok(0);
    }
    let probe_ok = std::process::Command::new("wsl")
        .args([
            "--",
            "bash",
            "-lc",
            "command -v bun && command -v cargo && echo READY",
        ])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("READY"))
        .unwrap_or(false);
    if !probe_ok {
        println!("[wsl] skipping (no distro with bun + cargo)");
        return Ok(-1);
    }
    let wsl_root = win_path_to_wsl(&root());
    let script = format!(
        "set -e\n\
         cd \"{wsl_root}\"\n\
         export CARGO_TARGET_DIR=\"$HOME/.cache/wtranscriber-wsl-target\"\n\
         export CARGO_INCREMENTAL=1\n\
         unset SHERPA_ONNX_LIB_DIR SHERPA_ONNX_LIB SHERPA_ONNX_INCLUDE_DIR\n\
         mkdir -p \"$CARGO_TARGET_DIR\"\n\
         bun install --frozen-lockfile --no-progress 2>&1 | tail -5\n\
         bun run tauri build --bundles deb -- --no-default-features --features sherpa-static\n"
    );
    run_streamed_stdin("wsl", "wsl", &["--", "bash", "-l"], &script, &[], lock)
}

fn find_host_bundle(ver: &str, branch: &str, dev: bool) -> Option<(PathBuf, String)> {
    let target = root()
        .join("src-tauri")
        .join("target")
        .join("release")
        .join("bundle");
    if is_windows() {
        let dir = target.join("nsis");
        if let Ok(entries) = fs::read_dir(&dir) {
            for e in entries.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) == Some("exe")
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.ends_with("-setup.exe"))
                        .unwrap_or(false)
                {
                    let name = if dev {
                        format!("wtranscriber-setup-{branch}.exe")
                    } else {
                        format!("wtranscriber-setup-{ver}.exe")
                    };
                    return Some((p, name));
                }
            }
        }
        return None;
    }
    let dir = target.join("deb");
    if let Ok(entries) = fs::read_dir(&dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("deb") {
                let name = if dev {
                    format!("wtranscriber-{branch}_amd64.deb")
                } else {
                    format!("wtranscriber_{ver}_amd64.deb")
                };
                return Some((p, name));
            }
        }
    }
    None
}

fn win_path_to_wsl(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    if s.len() >= 3 {
        let bytes = s.as_bytes();
        if bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/' {
            let drive = (bytes[0] as char).to_ascii_lowercase();
            return format!("/mnt/{}{}", drive, &s[2..]);
        }
    }
    s
}

fn find_wsl_deb(ver: &str, branch: &str, dev: bool) -> Option<(PathBuf, String)> {
    let probe = std::process::Command::new("wsl")
        .args([
            "--",
            "bash",
            "-lc",
            "ls \"$HOME/.cache/wtranscriber-wsl-target/release/bundle/deb/\"*.deb 2>/dev/null | head -1",
        ])
        .output()
        .ok()?;
    let wsl_path = String::from_utf8_lossy(&probe.stdout).trim().to_string();
    if wsl_path.is_empty() {
        return None;
    }
    let to_win = std::process::Command::new("wsl")
        .args(["--", "bash", "-c", &format!("wslpath -w '{wsl_path}'")])
        .output()
        .ok()?;
    let win_path = String::from_utf8_lossy(&to_win.stdout).trim().to_string();
    if win_path.is_empty() {
        return None;
    }
    let name = if dev {
        format!("wtranscriber-{branch}_amd64.deb")
    } else {
        format!("wtranscriber_{ver}_amd64.deb")
    };
    Some((PathBuf::from(win_path), name))
}

struct ApkResult {
    path: PathBuf,
    signed: bool,
}

fn find_apk(dev: bool) -> Result<Option<ApkResult>> {
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
    let signed = apk_dir.join("app-universal-release.apk");
    let unsigned = apk_dir.join("app-universal-release-unsigned.apk");
    if signed.exists() {
        let unsigned_newer = unsigned
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .zip(signed.metadata().ok().and_then(|m| m.modified().ok()))
            .map(|(u, s)| u > s)
            .unwrap_or(false);
        if !unsigned_newer {
            return Ok(Some(ApkResult {
                path: signed,
                signed: true,
            }));
        }
        let _ = fs::remove_file(&signed);
    }
    if !unsigned.exists() {
        return Ok(None);
    }

    if std::env::var_os("ANDROID_HOME").is_none() && !is_windows() {
        if let Some(home) = std::env::var_os("HOME") {
            let candidate = Path::new(&home).join("Android").join("Sdk");
            if candidate.exists() {
                unsafe { std::env::set_var("ANDROID_HOME", &candidate) };
                eprintln!(
                    "[and] defaulting ANDROID_HOME={} for apk signing",
                    candidate.display()
                );
            }
        }
    }
    let ks_props = root()
        .join("src-tauri")
        .join("gen")
        .join("android")
        .join("keystore.properties");
    let mut props: std::collections::HashMap<String, String> = if ks_props.exists() {
        fs::read_to_string(&ks_props)?
            .lines()
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .collect()
    } else if dev {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_default();
        let debug_ks = Path::new(&home).join(".android").join("debug.keystore");
        if !debug_ks.exists() {
            eprintln!(
                "⚠  no keystore.properties and no debug.keystore at {} — leaving APK unsigned",
                debug_ks.display()
            );
            return Ok(Some(ApkResult {
                path: unsigned,
                signed: false,
            }));
        }
        eprintln!(
            "[and] dev build: signing with debug keystore {}",
            debug_ks.display()
        );
        let mut p = std::collections::HashMap::new();
        p.insert(
            "storeFile".to_string(),
            debug_ks.to_string_lossy().to_string(),
        );
        p.insert("storePassword".to_string(), "android".to_string());
        p.insert("keyAlias".to_string(), "androiddebugkey".to_string());
        p.insert("keyPassword".to_string(), "android".to_string());
        p
    } else {
        return Ok(Some(ApkResult {
            path: unsigned,
            signed: false,
        }));
    };
    let _ = &mut props;
    let sdk = std::env::var("ANDROID_HOME").unwrap_or_else(|_| {
        let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
        format!("{local}\\Android\\Sdk")
    });
    let build_tools_dir = Path::new(&sdk).join("build-tools");
    let bt_ver = match fs::read_dir(&build_tools_dir) {
        Ok(rd) => {
            let mut versions: Vec<_> = rd
                .flatten()
                .filter_map(|e| e.file_name().into_string().ok())
                .collect();
            versions.sort();
            match versions.pop() {
                Some(v) => v,
                None => {
                    return Ok(Some(ApkResult {
                        path: unsigned,
                        signed: false,
                    }));
                }
            }
        }
        Err(_) => {
            return Ok(Some(ApkResult {
                path: unsigned,
                signed: false,
            }));
        }
    };
    let bt = build_tools_dir.join(bt_ver);
    let zipalign = bt.join(exe("zipalign"));
    let apksigner = if is_windows() {
        bt.join("apksigner.bat")
    } else {
        bt.join("apksigner")
    };
    let aligned = apk_dir.join("app-universal-release-aligned.apk");
    let out = apk_dir.join("app-universal-release.apk");
    let za = std::process::Command::new(&zipalign)
        .args([
            "-f",
            "-p",
            "4",
            unsigned.to_string_lossy().as_ref(),
            aligned.to_string_lossy().as_ref(),
        ])
        .status()?;
    if !za.success() {
        return Ok(Some(ApkResult {
            path: unsigned,
            signed: false,
        }));
    }
    let store_pass = format!(
        "pass:{}",
        props.get("storePassword").cloned().unwrap_or_default()
    );
    let key_pass = format!(
        "pass:{}",
        props.get("keyPassword").cloned().unwrap_or_default()
    );
    let aligned_str = aligned.to_string_lossy().to_string();
    let store_file = props.get("storeFile").cloned().unwrap_or_default();
    let alias = props.get("keyAlias").cloned().unwrap_or_default();
    let out_str = out.to_string_lossy().to_string();
    let sign_args: Vec<&str> = vec![
        "sign",
        "--ks",
        &store_file,
        "--ks-pass",
        &store_pass,
        "--ks-key-alias",
        &alias,
        "--key-pass",
        &key_pass,
        "--out",
        &out_str,
        &aligned_str,
    ];
    let s = std::process::Command::new(&apksigner)
        .args(&sign_args)
        .status()?;
    if !s.success() {
        return Ok(Some(ApkResult {
            path: unsigned,
            signed: false,
        }));
    }
    Ok(Some(ApkResult {
        path: out,
        signed: true,
    }))
}

fn copy_into_channel(src: &Path, name: &str, channel_dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(channel_dir)?;
    let dst = channel_dir.join(name);
    fs::copy(src, &dst).with_context(|| format!("copy {} -> {}", src.display(), dst.display()))?;
    let size = fs::metadata(&dst)?.len() as f64 / 1024.0 / 1024.0;
    println!("  + {} ({:.1} MB)", dst.display(), size);
    Ok(dst)
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
    Ok(())
}
