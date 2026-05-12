use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

use crate::util::root;

#[derive(Debug)]
pub(super) struct Abi {
    pub abi: &'static str,
    pub rust: &'static str,
    pub clang: &'static str,
}

pub(super) fn abi_for(target: &str) -> Result<Abi> {
    Ok(match target {
        "aarch64" => Abi {
            abi: "arm64-v8a",
            rust: "aarch64_linux_android",
            clang: "aarch64-linux-android24-clang",
        },
        "armv7" => Abi {
            abi: "armeabi-v7a",
            rust: "armv7_linux_androideabi",
            clang: "armv7a-linux-androideabi24-clang",
        },
        "i686" => Abi {
            abi: "x86",
            rust: "i686_linux_android",
            clang: "i686-linux-android24-clang",
        },
        "x86_64" => Abi {
            abi: "x86_64",
            rust: "x86_64_linux_android",
            clang: "x86_64-linux-android24-clang",
        },
        other => bail!("unknown target: {other} (expected: aarch64|armv7|i686|x86_64)"),
    })
}

pub(super) fn android_home() -> PathBuf {
    if let Ok(v) = std::env::var("ANDROID_HOME") {
        return PathBuf::from(v);
    }
    if cfg!(target_os = "windows") {
        PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_default())
            .join("Android")
            .join("Sdk")
    } else if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var("HOME").unwrap_or_default())
            .join("Library")
            .join("Android")
            .join("sdk")
    } else {
        PathBuf::from(std::env::var("HOME").unwrap_or_default())
            .join("Android")
            .join("Sdk")
    }
}

pub(super) fn ndk_home(android_home: &Path) -> PathBuf {
    if let Ok(v) = std::env::var("NDK_HOME") {
        return PathBuf::from(v);
    }
    android_home.join("ndk").join("27.2.12479018")
}

pub(super) fn ndk_bin(ndk: &Path) -> PathBuf {
    let host = if cfg!(target_os = "windows") {
        "windows-x86_64"
    } else if cfg!(target_os = "macos") {
        "darwin-x86_64"
    } else {
        "linux-x86_64"
    };
    ndk.join("toolchains")
        .join("llvm")
        .join("prebuilt")
        .join(host)
        .join("bin")
}

pub(super) fn clang_ext() -> &'static str {
    if cfg!(target_os = "windows") {
        ".cmd"
    } else {
        ""
    }
}

pub(super) fn gen_android() -> PathBuf {
    root().join("src-tauri").join("gen").join("android")
}

pub(super) fn apk_release_dir() -> PathBuf {
    gen_android()
        .join("app")
        .join("build")
        .join("outputs")
        .join("apk")
        .join("universal")
        .join("release")
}

pub(super) fn prebuilt_dir(target: &str) -> Result<PathBuf> {
    Ok(root()
        .join(".android-prebuilt")
        .join("jniLibs")
        .join(abi_for(target)?.abi))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_for_maps_supported_targets() {
        let cases = [
            (
                "aarch64",
                "arm64-v8a",
                "aarch64_linux_android",
                "aarch64-linux-android24-clang",
            ),
            (
                "armv7",
                "armeabi-v7a",
                "armv7_linux_androideabi",
                "armv7a-linux-androideabi24-clang",
            ),
            (
                "i686",
                "x86",
                "i686_linux_android",
                "i686-linux-android24-clang",
            ),
            (
                "x86_64",
                "x86_64",
                "x86_64_linux_android",
                "x86_64-linux-android24-clang",
            ),
        ];

        for (target, abi_name, rust, clang) in cases {
            let abi = abi_for(target).unwrap();
            assert_eq!(abi.abi, abi_name);
            assert_eq!(abi.rust, rust);
            assert_eq!(abi.clang, clang);
        }
    }

    #[test]
    fn abi_for_rejects_unknown_target() {
        let err = abi_for("mips").unwrap_err().to_string();

        assert!(err.contains("unknown target: mips"));
        assert!(err.contains("aarch64|armv7|i686|x86_64"));
    }
}
