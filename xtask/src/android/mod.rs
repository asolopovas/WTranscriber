use anyhow::Result;
use clap::{Args as ClapArgs, Subcommand, ValueEnum};

mod adb;
mod build;
pub(crate) mod dev;
mod lldb;
mod logs;
mod patch;
mod paths;
pub(crate) mod proc;

pub use patch::sign_patch_inline;

#[allow(dead_code)]
mod ident {
    include!("../../../shared/identity.rs");
}
const ANDROID_PACKAGE: &str = ident::APP_ID;

#[derive(Subcommand)]
#[command(about = "Android build / dev / bootstrap")]
pub enum Cmd {
    Build(TargetArgs),
    Dev(DevArgs),
    Bootstrap(BootstrapArgs),
}

#[derive(ClapArgs)]
pub struct TargetArgs {
    #[arg(long, default_value = "aarch64")]
    pub target: String,
}

#[derive(ClapArgs)]
pub struct DevArgs {
    #[arg(long)]
    pub open: bool,
    #[arg(long)]
    pub host: bool,
    #[arg(long)]
    pub watch: bool,
    pub device: Option<String>,
}

#[derive(Clone, ValueEnum)]
pub enum BootstrapMode {
    Usb,
    Host,
}

#[derive(ClapArgs)]
pub struct BootstrapArgs {
    #[arg(value_enum, default_value_t = BootstrapMode::Usb)]
    pub mode: BootstrapMode,
    pub device: Option<String>,
}

pub fn run(c: Cmd) -> Result<()> {
    match c {
        Cmd::Build(a) => build::cmd_build(&a.target),
        Cmd::Dev(a) => dev::cmd_dev(a.open, a.host, a.watch, a.device.as_deref()),
        Cmd::Bootstrap(a) => dev::cmd_bootstrap(a.mode, a.device.as_deref()),
    }
}
