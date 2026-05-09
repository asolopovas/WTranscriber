use anyhow::Result;
use clap::{Args as ClapArgs, Subcommand, ValueEnum};

mod adb;
mod build;
mod dev;
mod logs;
mod patch;
mod paths;
mod proc;

pub use patch::sign_patch_inline;

const ANDROID_PACKAGE: &str = "com.asolopovas.wtranscriber";

#[derive(Subcommand)]
#[command(about = "Android build / dev / doctor / prebuilts / sign-patch / cli helpers")]
pub enum Cmd {
    Build(TargetArgs),
    Install(InstallArgs),
    Dev(DevArgs),
    Bootstrap(BootstrapArgs),
    Status(StatusArgs),
    Stop(StopArgs),
    Attach(AttachArgs),
    Smoke(AttachArgs),
    Doctor(TargetArgs),
    Cli(CliArgs),
    CliPush,
    CliRun {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    Prebuilts {
        #[arg(default_value = "")]
        version: String,
    },
    SignPatch,
}

#[derive(ClapArgs)]
pub struct TargetArgs {
    #[arg(long, default_value = "aarch64")]
    pub target: String,
}

#[derive(ClapArgs)]
pub struct InstallArgs {
    #[arg(long, default_value = "aarch64")]
    pub target: String,
    #[arg(long)]
    pub fresh: bool,
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

#[derive(ClapArgs)]
pub struct StatusArgs {
    #[arg(long)]
    pub json: bool,
    pub device: Option<String>,
}

#[derive(ClapArgs)]
pub struct StopArgs {
    #[arg(long)]
    pub keep_reverse: bool,
    pub device: Option<String>,
}

#[derive(ClapArgs)]
pub struct AttachArgs {
    pub device: Option<String>,
}

#[derive(ClapArgs)]
pub struct CliArgs {
    #[arg(long, default_value = "aarch64")]
    pub target: String,
    #[arg(long)]
    pub debug: bool,
}

pub fn run(c: Cmd) -> Result<()> {
    match c {
        Cmd::Build(a) => build::cmd_build(&a.target),
        Cmd::Install(a) => build::cmd_install(&a.target, a.fresh),
        Cmd::Dev(a) => dev::cmd_dev(a.open, a.host, a.watch, a.device.as_deref()),
        Cmd::Bootstrap(a) => dev::cmd_bootstrap(a.mode, a.device.as_deref()),
        Cmd::Status(a) => dev::cmd_status(a.json, a.device.as_deref()),
        Cmd::Stop(a) => dev::cmd_stop(a.keep_reverse, a.device.as_deref()),
        Cmd::Attach(a) => adb::attach_webview(a.device.as_deref(), false),
        Cmd::Smoke(a) => dev::cmd_smoke(a.device.as_deref()),
        Cmd::Doctor(a) => build::cmd_doctor(&a.target),
        Cmd::Cli(a) => build::cmd_cli(&a.target, a.debug),
        Cmd::CliPush => build::cmd_cli_push(),
        Cmd::CliRun { args } => build::cmd_cli_run(&args),
        Cmd::Prebuilts { version } => {
            build::cmd_prebuilts((!version.is_empty()).then_some(version))
        }
        Cmd::SignPatch => patch::sign_patch_inline().map(|_| ()),
    }
}
