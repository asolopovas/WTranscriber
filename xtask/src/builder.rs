use anyhow::{Result, bail};
use clap::{Args as ClapArgs, Subcommand};

use crate::util::{run_streamed, shared_out};

const DEFAULT_IMAGE: &str = "asolopovas/tauri-builder";
const DEFAULT_TAG: &str = "debian12";
const DOCKERFILE: &str = "Dockerfile.builder";

#[derive(Subcommand)]
#[command(about = "Build and publish the reusable Tauri builder Docker image")]
pub enum Cmd {
    /// Build the builder image locally from Dockerfile.builder.
    Build(Args),
    /// Push the builder image tags to the registry (run `docker login` first).
    Push(Args),
    /// Build then push the builder image (build + push).
    Publish(Args),
}

#[derive(ClapArgs, Clone)]
pub struct Args {
    /// Image repository, e.g. asolopovas/tauri-builder.
    #[arg(long, default_value = DEFAULT_IMAGE)]
    pub image: String,
    /// Primary tag; the image is also tagged `latest` unless --no-latest.
    #[arg(long, default_value = DEFAULT_TAG)]
    pub tag: String,
    /// Do not additionally tag/push `latest`.
    #[arg(long)]
    pub no_latest: bool,
}

impl Args {
    fn refs(&self) -> Vec<String> {
        let mut refs = vec![format!("{}:{}", self.image, self.tag)];
        if !self.no_latest {
            refs.push(format!("{}:latest", self.image));
        }
        refs
    }
}

pub fn run(cmd: Cmd) -> Result<()> {
    match cmd {
        Cmd::Build(a) => build(&a),
        Cmd::Push(a) => push(&a),
        Cmd::Publish(a) => {
            build(&a)?;
            push(&a)
        }
    }
}

fn build(args: &Args) -> Result<()> {
    let lock = shared_out();
    let refs = args.refs();
    let mut argv = vec!["build", "-f", DOCKERFILE];
    for r in &refs {
        argv.push("-t");
        argv.push(r);
    }
    argv.push(".");
    println!("[builder] building {}", refs.join(", "));
    let rc = run_streamed("builder", "docker", &argv, &[], &lock)?;
    if rc != 0 {
        bail!("docker build failed (exit {rc})");
    }
    println!("[builder] built {}", refs.join(", "));
    Ok(())
}

fn push(args: &Args) -> Result<()> {
    let lock = shared_out();
    for r in &args.refs() {
        println!("[builder] pushing {r}");
        let rc = run_streamed("builder", "docker", &["push", r], &[], &lock)?;
        if rc != 0 {
            bail!(
                "docker push {r} failed (exit {rc}); run `docker login` and ensure the repo exists and is public"
            );
        }
    }
    println!("[builder] pushed {}", args.refs().join(", "));
    Ok(())
}
