use anyhow::Result;
use clap::Args as ClapArgs;

use crate::{bump, check, publish, release};

#[derive(ClapArgs)]
#[command(about = "Run the stable local release flow")]
pub struct Args {
    #[arg(default_value = "patch")]
    pub level: String,
    #[arg(long)]
    pub skip_check: bool,
}

pub fn run(args: Args) -> Result<()> {
    if !args.skip_check {
        check::run(check::Args {
            sequential: false,
            jobs: Vec::new(),
        })?;
    }
    bump::run(bump::Args { level: args.level })?;
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
