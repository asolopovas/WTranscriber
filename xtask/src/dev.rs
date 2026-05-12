use anyhow::Result;
use clap::Subcommand;

use crate::android;
use crate::android::proc::{kill_pid, port_owner};

#[derive(Subcommand)]
#[command(about = "Cross-platform dev session control")]
pub enum Cmd {
    Stop,
}

pub fn run(c: Cmd) -> Result<()> {
    match c {
        Cmd::Stop => cmd_stop(),
    }
}

fn cmd_stop() -> Result<()> {
    let _ = android::dev::cmd_stop(false, None);
    for port in [1420u16, 1421u16] {
        if let Some(pid) = port_owner(port) {
            kill_pid(pid);
            println!("stopped :{port} owner pid={pid}");
        }
    }
    println!("dev stopped");
    Ok(())
}
