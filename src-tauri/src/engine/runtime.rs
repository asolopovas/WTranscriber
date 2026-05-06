use crate::config::{Config, Device};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Cpu,
    Cuda,
}

impl Provider {
    pub const fn as_arg(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Cuda => "cuda",
        }
    }
}

pub const fn provider(config: &Config) -> Provider {
    match config.device {
        Device::Cuda => Provider::Cuda,
        Device::Cpu => Provider::Cpu,
    }
}

pub const fn threads(config: &Config) -> u32 {
    if config.threads > 0 { config.threads } else { 4 }
}
