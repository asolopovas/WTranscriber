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

pub fn provider(config: &Config) -> Provider {
    match crate::runtimes::dependencies::onnx_provider(config.device) {
        "cuda" => Provider::Cuda,
        _ => Provider::Cpu,
    }
}

pub const fn threads(config: &Config) -> u32 {
    let requested = if config.threads > 0 {
        config.threads
    } else {
        4
    };
    match config.device {
        Device::Cuda => {
            if requested > 2 {
                2
            } else {
                requested
            }
        }
        Device::Cpu => requested,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Engine;

    #[test]
    fn cuda_provider_follows_build_support() {
        let cfg = Config {
            engine: Engine::Parakeet,
            device: Device::Cuda,
            ..Default::default()
        };
        let expected = if crate::runtimes::dependencies::onnx_cuda_supported_for_build() {
            Provider::Cuda
        } else {
            Provider::Cpu
        };
        assert_eq!(provider(&cfg), expected);
    }
}
