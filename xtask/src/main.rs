use anyhow::Result;
use clap::Parser;

mod android;
mod bump;
mod check;
mod cuda_workers;
mod dev;
mod publish;
mod release;
mod release_stable;
mod util;

#[derive(Parser)]
#[command(name = "xtask", about = "WTranscriber build/release orchestration")]
enum Cmd {
    Release(release::Args),
    Bump(bump::Args),
    Publish(publish::Args),
    Check(check::Args),
    ReleaseStable(release_stable::Args),
    #[command(subcommand)]
    Android(android::Cmd),
    #[command(subcommand)]
    CudaWorkers(cuda_workers::Cmd),
    #[command(subcommand)]
    Dev(dev::Cmd),
}

fn main() -> Result<()> {
    match Cmd::parse() {
        Cmd::Release(a) => release::run(a),
        Cmd::Bump(a) => bump::run(a),
        Cmd::Publish(a) => publish::run(a),
        Cmd::Check(a) => check::run(a),
        Cmd::ReleaseStable(a) => release_stable::run(a),
        Cmd::Android(c) => android::run(c),
        Cmd::CudaWorkers(c) => cuda_workers::run(c),
        Cmd::Dev(c) => dev::run(c),
    }
}
