use std::path::PathBuf;

use crate::{
    config::Device,
    error::{Error, Result},
    models::download::{Progress, download_file},
    paths,
    runtimes::{ensure_cache_subdir, extract, locate_bin_dir, move_or_copy_dir},
};

pub const VERSION: &str = include_str!("../../sherpa-version.txt").trim_ascii_end();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Cpu,
    Cuda,
}

impl Variant {
    pub const fn from_device(d: Device) -> Self {
        match d {
            Device::Cuda => Self::Cuda,
            Device::Cpu => Self::Cpu,
        }
    }

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Cuda => "cuda",
        }
    }

    pub fn asset_name(self) -> Option<String> {
        let v = VERSION;
        let name = if cfg!(target_os = "windows") && cfg!(target_arch = "x86_64") {
            match self {
                Self::Cpu => format!("sherpa-onnx-{v}-win-x64-shared-MD-Release-no-tts.tar.bz2"),
                Self::Cuda => {
                    format!("sherpa-onnx-{v}-cuda-12.x-cudnn-9.x-win-x64-cuda.tar.bz2")
                }
            }
        } else if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
            match self {
                Self::Cpu => format!("sherpa-onnx-{v}-linux-x64-shared.tar.bz2"),
                Self::Cuda => format!("sherpa-onnx-{v}-cuda-12.x-cudnn-9.x-linux-x64-gpu.tar.bz2"),
            }
        } else if cfg!(target_os = "macos") {
            if cfg!(target_arch = "aarch64") {
                format!("sherpa-onnx-{v}-osx-arm64-shared.tar.bz2")
            } else {
                format!("sherpa-onnx-{v}-osx-x64-shared.tar.bz2")
            }
        } else {
            return None;
        };
        Some(name)
    }

    pub fn url(self) -> Option<String> {
        let asset = self.asset_name()?;
        Some(format!(
            "https://github.com/k2-fsa/sherpa-onnx/releases/download/{VERSION}/{asset}"
        ))
    }
}

pub fn root_dir() -> Result<PathBuf> {
    let d = paths::third_party_dir()?.join("sherpa-onnx");
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

pub fn install_dir(variant: Variant) -> Result<PathBuf> {
    let d = root_dir()?.join(format!("{}-{}", VERSION, variant.slug()));
    Ok(d)
}

pub fn bin_dir(variant: Variant) -> Result<PathBuf> {
    Ok(install_dir(variant)?.join("bin"))
}

pub fn binary_path(variant: Variant, name: &str) -> Result<PathBuf> {
    Ok(bin_dir(variant)?.join(name))
}

pub fn is_installed(variant: Variant) -> bool {
    let name = if cfg!(windows) {
        "sherpa-onnx-offline.exe"
    } else {
        "sherpa-onnx-offline"
    };
    binary_path(variant, name).is_ok_and(|p| p.exists())
}

pub fn find_in_install_dir(variant: Variant, name: &str) -> Option<PathBuf> {
    let p = binary_path(variant, name).ok()?;
    p.exists().then_some(p)
}

pub fn find_any(name: &str) -> Option<PathBuf> {
    for v in [Variant::Cuda, Variant::Cpu] {
        if let Some(p) = find_in_install_dir(v, name) {
            return Some(p);
        }
    }
    None
}

pub async fn ensure(
    variant: Variant,
    on_progress: &mut (dyn FnMut(Progress) + Send),
) -> Result<PathBuf> {
    let dir = install_dir(variant)?;
    if is_installed(variant) {
        return Ok(dir);
    }
    let url = variant
        .url()
        .ok_or_else(|| Error::Config("no sherpa-onnx asset for this platform".into()))?;
    let asset = variant
        .asset_name()
        .expect("asset_name is Some when url() is Some");

    let cache = ensure_cache_subdir("sherpa-onnx")?;
    let tarball = cache.join(&asset);

    if !tarball.exists() {
        download_file(&tarball, &url, None, on_progress).await?;
    }

    let staging = cache.join(format!("staging-{}", variant.slug()));
    if staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    std::fs::create_dir_all(&staging)?;

    extract(&tarball, &staging)?;

    let target = if cfg!(windows) {
        "sherpa-onnx-offline.exe"
    } else {
        "sherpa-onnx-offline"
    };
    let bin_src = locate_bin_dir(&staging, target).ok_or_else(|| {
        Error::Config(format!(
            "sherpa-onnx archive layout unexpected (no bin/ found in {})",
            staging.display()
        ))
    })?;

    let target_bin = bin_dir(variant)?;
    move_or_copy_dir(&bin_src, &target_bin)?;

    if !cfg!(windows)
        && let Some(lib_src) = bin_src.parent().map(|p| p.join("lib"))
        && lib_src.is_dir()
    {
        let target_lib = install_dir(variant)?.join("lib");
        move_or_copy_dir(&lib_src, &target_lib)?;
    }

    let _ = std::fs::remove_dir_all(&staging);
    Ok(dir)
}
