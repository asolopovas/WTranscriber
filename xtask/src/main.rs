use anyhow::Result;
use clap::Parser;

mod android;
mod bump;
mod dev;
mod publish;
mod release;
mod util;

#[derive(Parser)]
#[command(name = "xtask", about = "WTranscriber build/release orchestration")]
enum Cmd {
    Release(release::Args),
    Bump(bump::Args),
    Publish(publish::Args),
    #[command(subcommand)]
    Android(android::Cmd),
    #[command(subcommand)]
    Dev(dev::Cmd),
}

fn main() -> Result<()> {
    match Cmd::parse() {
        Cmd::Release(a) => release::run(a),
        Cmd::Bump(a) => bump::run(a),
        Cmd::Publish(a) => publish::run(a),
        Cmd::Android(c) => android::run(c),
        Cmd::Dev(c) => dev::run(c),
    }
}
