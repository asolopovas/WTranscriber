use anyhow::{Result, bail};
use clap::Args as ClapArgs;
use std::process::{Command, Stdio};

use crate::util::{capture, pkg_version, root, sh};
use crate::{bump, check, publish, release};

#[derive(ClapArgs)]
#[command(about = "Run the stable local release flow")]
pub struct Args {
    #[arg(long, num_args = 0..=1, default_missing_value = "patch", value_name = "LEVEL")]
    pub bump: Option<String>,
    #[arg(long)]
    pub skip_check: bool,
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
    ensure_clean()?;
    preflight(&args)?;
    if !args.skip_check {
        check::run(check::Args {
            sequential: false,
            jobs: Vec::new(),
        })?;
    }
    if let Some(level) = args.bump {
        bump::run(bump::Args { level })?;
    }
    sync_current_version_tag()?;
    release::run(release::Args {
        dev: false,
        no_host: false,
        no_android: args.no_android,
        no_deb: args.no_deb,
        no_windows_vm: args.no_windows_vm,
        skip_rebuild: args.skip_rebuild,
        sequential: args.sequential,
    })?;
    publish::run(publish::Args {
        channel: "stable".into(),
    })?;
    Ok(())
}

fn ensure_clean() -> Result<()> {
    let dirty = capture("git", &["status", "--porcelain"])?;
    if !dirty.is_empty() {
        bail!("working tree is dirty; commit or stash first");
    }
    Ok(())
}

fn preflight(args: &Args) -> Result<()> {
    ensure_gh_authenticated()?;
    ensure_not_behind_upstream()?;
    if !args.no_android {
        let keystore = root()
            .join("src-tauri")
            .join("gen")
            .join("android")
            .join("keystore.properties");
        if !keystore.exists() {
            bail!(
                "stable releases require a signed APK but {} is missing; \
                 configure Android signing (docs/release.md) or pass --no-android",
                keystore.display()
            );
        }
    }
    let android_needs_docker = !args.no_android && std::env::var_os("WT_ANDROID_NATIVE").is_none();
    if (!args.no_deb || android_needs_docker) && !docker_ready() {
        let needs: Vec<&str> = [
            (!args.no_deb).then_some("linux .deb"),
            android_needs_docker.then_some("android apk"),
        ]
        .into_iter()
        .flatten()
        .collect();
        bail!(
            "docker engine is not reachable (needed for: {}); \
             start Docker Desktop, or pass --no-deb / --no-android",
            needs.join(", ")
        );
    }
    Ok(())
}

fn ensure_gh_authenticated() -> Result<()> {
    let status = Command::new("gh")
        .args(["auth", "status"])
        .current_dir(root())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => bail!("gh CLI is not authenticated; run `gh auth login` before releasing"),
        Err(_) => bail!("gh CLI not found; install GitHub CLI before releasing"),
    }
}

fn ensure_not_behind_upstream() -> Result<()> {
    sh("git", &["fetch", "origin", "--quiet"])?;
    let Ok(behind) = capture("git", &["rev-list", "--count", "HEAD..@{upstream}"]) else {
        println!("release-stable: no upstream configured; skipping behind check");
        return Ok(());
    };
    if behind.trim() != "0" {
        bail!(
            "branch is behind its upstream by {} commit(s); pull or rebase before releasing",
            behind.trim()
        );
    }
    Ok(())
}

fn docker_ready() -> bool {
    Command::new("docker")
        .args(["info", "--format", "{{.ServerVersion}}"])
        .current_dir(root())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn sync_current_version_tag() -> Result<()> {
    let ver = pkg_version()?;
    let version_tag = format!("v{ver}");
    let head = capture("git", &["rev-parse", "HEAD"])?;
    sync_version_tag_to_head(&version_tag, &format!("Release {version_tag}"), &head)?;
    sync_latest_tag_to_head(&version_tag, &head)
}

fn sync_version_tag_to_head(tag: &str, message: &str, head: &str) -> Result<()> {
    let tag_commit = tag_commit(tag)?;
    match tag_commit.as_deref() {
        Some(commit) if commit == head => println!("release-stable: {tag} already points at HEAD"),
        Some(commit) => bail!(
            "stable tag {tag} already points to {commit}, not HEAD {head}; bump the version instead"
        ),
        None => {
            println!("release-stable: creating {tag} at HEAD ({head})");
            retag_head(tag, message)?;
        }
    }
    Ok(())
}

fn sync_latest_tag_to_head(version_tag: &str, head: &str) -> Result<()> {
    let message = format!("Latest release ({version_tag})");
    let tag_commit = tag_commit("latest")?;
    match tag_commit.as_deref() {
        Some(commit) if commit == head => println!("release-stable: latest already points at HEAD"),
        Some(commit) => {
            println!("release-stable: moving latest from {commit} to HEAD ({head})");
            retag_head("latest", &message)?;
        }
        None => {
            println!("release-stable: creating latest at HEAD ({head})");
            retag_head("latest", &message)?;
        }
    }
    Ok(())
}

fn tag_commit(tag: &str) -> Result<Option<String>> {
    let output = Command::new("git")
        .args(["rev-parse", &format!("{tag}^{{commit}}")])
        .current_dir(root())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()?;
    if output.status.success() {
        Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        ))
    } else if output.status.code() == Some(128) {
        Ok(None)
    } else {
        bail!("git rev-parse failed for {tag}");
    }
}

fn retag_head(tag: &str, message: &str) -> Result<()> {
    sh("git", &["tag", "-f", "-a", tag, "HEAD", "-m", message])
}
