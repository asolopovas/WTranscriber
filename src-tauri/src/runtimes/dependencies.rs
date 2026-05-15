use crate::config::Device;

use super::sherpa::Variant as SherpaVariant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    pub requested: Device,
    pub runtime_device: Device,
    pub sherpa: SherpaVariant,
    pub onnx_cuda: bool,
    pub cudnn: bool,
}

impl Plan {
    pub const fn setup_process_cuda(self) -> bool {
        matches!(self.runtime_device, Device::Cuda)
    }

    pub const fn cuda_without_gpu(self) -> bool {
        matches!(self.requested, Device::Cuda) && matches!(self.runtime_device, Device::Cpu)
    }

    pub const fn onnx_cuda_unavailable(self) -> bool {
        self.setup_process_cuda() && !self.onnx_cuda
    }
}

pub const fn onnx_cuda_supported_for_build() -> bool {
    cfg!(feature = "cuda")
        && !cfg!(all(target_os = "windows", feature = "directml"))
        && cfg!(any(target_os = "linux", target_os = "windows"))
}

pub const fn onnx_provider(device: Device) -> &'static str {
    if matches!(device, Device::Cuda) && onnx_cuda_supported_for_build() {
        "cuda"
    } else {
        "cpu"
    }
}

pub const fn plan(requested: Device, gpu_present: bool, cudnn_supported: bool) -> Plan {
    let cuda_without_gpu = matches!(requested, Device::Cuda) && !gpu_present;
    let runtime_device = if cuda_without_gpu {
        Device::Cpu
    } else {
        requested
    };
    let onnx_cuda = matches!(runtime_device, Device::Cuda) && onnx_cuda_supported_for_build();
    Plan {
        requested,
        runtime_device,
        sherpa: if onnx_cuda {
            SherpaVariant::Cuda
        } else {
            SherpaVariant::Cpu
        },
        onnx_cuda,
        cudnn: onnx_cuda && cudnn_supported,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_plan_uses_cpu_sherpa() {
        let plan = plan(Device::Cpu, true, true);
        assert_eq!(plan.sherpa, SherpaVariant::Cpu);
        assert!(!plan.onnx_cuda);
        assert!(!plan.cudnn);
        assert!(!plan.setup_process_cuda());
    }

    #[test]
    fn cuda_without_gpu_uses_cpu_runtime() {
        let plan = plan(Device::Cuda, false, true);
        assert_eq!(plan.runtime_device, Device::Cpu);
        assert!(plan.cuda_without_gpu());
        assert_eq!(plan.sherpa, SherpaVariant::Cpu);
        assert!(!plan.setup_process_cuda());
    }

    #[test]
    fn cuda_plan_follows_build_support() {
        let plan = plan(Device::Cuda, true, true);
        assert_eq!(plan.onnx_cuda, onnx_cuda_supported_for_build());
        assert_eq!(plan.cudnn, onnx_cuda_supported_for_build());
        assert_eq!(
            onnx_provider(Device::Cuda),
            if plan.onnx_cuda { "cuda" } else { "cpu" }
        );
    }
}
