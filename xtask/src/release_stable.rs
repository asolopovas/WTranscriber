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
}

pub fn run(args: Args) -> Result<()> {
    ensure_clean()?;
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
        no_android: false,
        no_deb: false,
        no_windows_vm: false,
        skip_rebuild: false,
        sequential: false,
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

fn sync_current_version_tag() -> Result<()> {
    let ver = pkg_version()?;
    let version_tag = format!("v{ver}");
    let head = capture("git", &["rev-parse", "HEAD"])?;
    sync_tag_to_head(&version_tag, &format!("Release {version_tag}"), &head)?;
    sync_tag_to_head("latest", &format!("Latest release ({version_tag})"), &head)
}

fn sync_tag_to_head(tag: &str, message: &str, head: &str) -> Result<()> {
    let tag_commit = tag_commit(tag)?;
    match tag_commit.as_deref() {
        Some(commit) if commit == head => println!("release-stable: {tag} already points at HEAD"),
        Some(commit) => {
            println!("release-stable: moving {tag} from {commit} to HEAD ({head})");
            retag_head(tag, message)?;
        }
        None => {
            println!("release-stable: creating {tag} at HEAD ({head})");
            retag_head(tag, message)?;
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
