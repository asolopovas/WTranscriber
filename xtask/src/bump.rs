use anyhow::{Context, Result, bail};
use clap::Args as ClapArgs;
use std::fs;

use crate::util::{capture, pkg_version, root, set_json_string, sh};

#[derive(ClapArgs)]
#[command(about = "Bump version, commit, tag (no push, no build)")]
pub struct Args {
    #[arg(default_value = "patch")]
    pub level: String,
}

pub fn run(args: Args) -> Result<()> {
    ensure_clean()?;
    let cur = pkg_version()?;
    let next = compute_next(&cur, &args.level)?;
    let tag = format!("v{next}");
    if tag_exists(&tag) {
        bail!("tag {tag} already exists");
    }
    println!("bump: {cur} -> {next}");
    set_pkg_version(&next)?;
    set_cargo_version(&next)?;
    set_tauri_version(&next)?;
    let have_lock = sync_cargo_lock()?;

    let mut to_add: Vec<&str> = vec![
        "package.json",
        "src-tauri/Cargo.toml",
        "src-tauri/tauri.conf.json",
    ];
    if have_lock {
        to_add.push("src-tauri/Cargo.lock");
    }
    let mut args_add: Vec<&str> = vec!["add"];
    args_add.extend(to_add);
    sh("git", &args_add)?;
    let msg = format!("chore(release): {next}");
    sh("git", &["commit", "--no-verify", "-m", &msg])?;
    let tag_msg = format!("Release {tag}");
    sh("git", &["tag", "-a", &tag, "-m", &tag_msg])?;
    println!("tagged {tag}");
    Ok(())
}

fn ensure_clean() -> Result<()> {
    let dirty = capture("git", &["status", "--porcelain"])?;
    if !dirty.is_empty() {
        bail!("working tree is dirty; commit or stash first");
    }
    Ok(())
}

fn tag_exists(tag: &str) -> bool {
    std::process::Command::new("git")
        .args(["rev-parse", tag])
        .current_dir(root())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn compute_next(cur: &str, level: &str) -> Result<String> {
    let parse_xyz = |s: &str| -> Result<(u32, u32, u32)> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            bail!("version {s} is not strict X.Y.Z");
        }
        Ok((parts[0].parse()?, parts[1].parse()?, parts[2].parse()?))
    };
    let (x, y, z) = parse_xyz(cur)?;
    Ok(match level {
        "patch" => format!("{x}.{y}.{}", z + 1),
        "minor" => format!("{x}.{}.0", y + 1),
        "major" => format!("{}.0.0", x + 1),
        explicit => {
            let _ = parse_xyz(explicit).with_context(|| {
                format!("invalid level {explicit:?} (expected patch|minor|major|X.Y.Z)")
            })?;
            explicit.to_string()
        }
    })
}

fn set_pkg_version(v: &str) -> Result<()> {
    set_json_string(&root().join("package.json"), "version", v)
}

fn set_cargo_version(v: &str) -> Result<()> {
    let p = root().join("src-tauri").join("Cargo.toml");
    let raw = fs::read_to_string(&p)?;
    let mut found = false;
    let mut out = String::with_capacity(raw.len());
    for line in raw.split_inclusive('\n') {
        if !found {
            let trimmed = line.trim_start();
            if trimmed.starts_with("version") && trimmed.contains('=') {
                let eol = if line.ends_with("\r\n") {
                    "\r\n"
                } else if line.ends_with('\n') {
                    "\n"
                } else {
                    ""
                };
                out.push_str(&format!("version = \"{v}\"{eol}"));
                found = true;
                continue;
            }
        }
        out.push_str(line);
    }
    if !found {
        bail!("could not find `version = ...` line in src-tauri/Cargo.toml");
    }
    fs::write(&p, out)?;
    Ok(())
}

fn set_tauri_version(v: &str) -> Result<()> {
    set_json_string(
        &root().join("src-tauri").join("tauri.conf.json"),
        "version",
        v,
    )
}

fn sync_cargo_lock() -> Result<bool> {
    let lock = root().join("src-tauri").join("Cargo.lock");
    if !lock.exists() {
        return Ok(false);
    }
    sh(
        "cargo",
        &[
            "update",
            "--manifest-path",
            "src-tauri/Cargo.toml",
            "-w",
            "--offline",
        ],
    )?;
    Ok(true)
}
