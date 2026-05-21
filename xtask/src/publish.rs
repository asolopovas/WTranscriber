use anyhow::{Context, Result, bail};
use clap::Args as ClapArgs;
use std::fs;
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;

use crate::util::{capture, pkg_version, root, sh};

#[derive(ClapArgs)]
#[command(about = "Publish artifacts produced by `xtask release` to a GitHub release")]
pub struct Args {
    pub channel: String,
}

pub fn run(args: Args) -> Result<()> {
    if !["dev", "stable"].contains(&args.channel.as_str()) {
        bail!("channel must be 'dev' or 'stable'");
    }
    let dev = args.channel == "dev";

    ensure_gh_config_dir();

    let list_file = root().join("releases").join(if dev {
        ".release-dev-artifacts"
    } else {
        ".release-stable-artifacts"
    });
    if !list_file.exists() {
        bail!(
            "{} not found — run `xtask release` first",
            list_file.display()
        );
    }
    let artifacts: Vec<PathBuf> = fs::read_to_string(&list_file)?
        .lines()
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .collect();
    if artifacts.is_empty() {
        bail!("no artifacts in {}", list_file.display());
    }
    println!(
        "artifacts: {}",
        artifacts
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );

    if which("gh").is_err() {
        bail!("gh CLI not found");
    }

    if dev {
        publish_dev(&artifacts)?;
    } else {
        publish_stable(&artifacts)?;
    }
    Ok(())
}

fn publish_dev(artifacts: &[PathBuf]) -> Result<()> {
    let sha = capture("git", &["rev-parse", "--short", "HEAD"])?;
    let mut branch = capture("git", &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if branch == "HEAD" {
        branch = "main".into();
    }
    let tag = "dev";

    println!("--- updating {tag} tag to {sha} ---");
    sh("git", &["push", "origin", "HEAD"])?;
    sh("git", &["tag", "-f", tag, "HEAD"])?;
    sh("git", &["push", "origin", tag, "--force"])?;

    if release_exists(tag) {
        println!("--- deleting existing {tag} release ---");
        sh(
            "gh",
            &["release", "delete", tag, "--yes", "--cleanup-tag=false"],
        )?;
    }
    println!("--- creating {tag} prerelease ---");
    let title = format!("Dev ({branch} @ {sha})");
    let notes = format!(
        "Rolling dev build of `{branch}` at commit `{sha}`. Artifacts replaced on every \
         `just release`. Not a stable release; APK may be unsigned. SHA256SUMS attached."
    );
    sh(
        "gh",
        &[
            "release",
            "create",
            tag,
            "--title",
            &title,
            "--prerelease",
            "--latest=false",
            "--notes",
            &notes,
        ],
    )?;
    upload_with_retry(tag, artifacts)?;
    println!("✓ dev: https://github.com/asolopovas/WTranscriber/releases/tag/{tag}");
    Ok(())
}

fn publish_stable(artifacts: &[PathBuf]) -> Result<()> {
    let ver = pkg_version()?;
    let tag = format!("v{ver}");
    let dirty = capture("git", &["status", "--porcelain"])?;
    if !dirty.is_empty() {
        bail!("working tree dirty — refusing to publish stable");
    }
    let head = capture("git", &["rev-parse", "HEAD"])?;
    let tag_commit = std::process::Command::new("git")
        .args(["rev-parse", &format!("{tag}^{{commit}}")])
        .current_dir(root())
        .output()
        .context("spawn git")?;
    if !tag_commit.status.success() {
        bail!(
            "tag {tag} does not exist locally — run `just release --stable` or `cargo xtask bump` first"
        );
    }
    let tag_commit = String::from_utf8_lossy(&tag_commit.stdout)
        .trim()
        .to_string();
    if tag_commit != head {
        bail!(
            "tag {tag} points to {tag_commit}, not HEAD {head} — bump the version before publishing stable"
        );
    }
    let latest_commit = std::process::Command::new("git")
        .args(["rev-parse", "latest^{commit}"])
        .current_dir(root())
        .output()
        .context("spawn git")?;
    if !latest_commit.status.success() {
        bail!(
            "tag latest does not exist locally — run `just release --stable` to sync stable tags"
        );
    }
    let latest_commit = String::from_utf8_lossy(&latest_commit.stdout)
        .trim()
        .to_string();
    if latest_commit != head {
        bail!(
            "tag latest points to {latest_commit}, not HEAD {head} — run `just release --stable` to sync stable tags"
        );
    }
    println!("--- pushing HEAD + tags {tag}, latest ---");
    sh("git", &["push", "origin", "HEAD"])?;
    let tag_ref = format!("refs/tags/{tag}:refs/tags/{tag}");
    sh("git", &["push", "--force", "origin", &tag_ref])?;
    sh(
        "git",
        &[
            "push",
            "--force",
            "origin",
            "refs/tags/latest:refs/tags/latest",
        ],
    )?;

    if release_exists(&tag) {
        println!("--- release {tag} already exists; uploading additional artifacts ---");
        sh("gh", &["release", "edit", &tag, "--latest"])?;
    } else {
        println!("--- creating release {tag} ---");
        sh(
            "gh",
            &[
                "release",
                "create",
                &tag,
                "--title",
                &tag,
                "--generate-notes",
                "--latest",
            ],
        )?;
    }
    upload_with_retry(&tag, artifacts)?;
    println!("✓ stable: https://github.com/asolopovas/WTranscriber/releases/tag/{tag}");
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

fn upload_with_retry(tag: &str, artifacts: &[PathBuf]) -> Result<()> {
    for attempt in 1..=3u32 {
        let mut args: Vec<String> = vec!["release".into(), "upload".into(), tag.into()];
        for p in artifacts {
            args.push(p.to_string_lossy().to_string());
        }
        args.push("--clobber".into());
        let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let status = std::process::Command::new("gh")
            .args(&arg_refs)
            .current_dir(root())
            .status()
            .context("spawn gh")?;
        if status.success() {
            return Ok(());
        }
        if attempt == 3 {
            bail!("gh release upload failed after 3 attempts");
        }
        eprintln!("upload attempt {attempt} failed; retrying in 5s...");
        sleep(Duration::from_secs(5));
    }
    Ok(())
}

fn which(name: &str) -> Result<PathBuf> {
    let out = std::process::Command::new(if cfg!(windows) { "where" } else { "which" })
        .arg(name)
        .output()?;
    if !out.status.success() {
        bail!("{name} not found");
    }
    let line = String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .to_string();
    Ok(PathBuf::from(line))
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
