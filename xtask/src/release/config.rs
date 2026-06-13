use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::util::{read_json, root};

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(super) struct ReleaseConfig {
    pub windows_vm: WindowsVmConfig,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(super) struct WindowsVmConfig {
    pub ssh_host: String,
    pub vm_dir: String,
    pub start_command: Vec<String>,
    pub restart_command: Vec<String>,
    pub ssh_ready_command: String,
    pub remote_work_dir: String,
    pub remote_repo_dir: String,
    pub helper_remote_path: String,
}

impl ReleaseConfig {
    pub fn load() -> Result<Self> {
        let path = if let Ok(custom) = std::env::var("WT_RELEASE_CONFIG") {
            PathBuf::from(custom)
        } else {
            let local = root().join("release.config.local.json");
            if local.exists() {
                local
            } else {
                root().join("release.config.json")
            }
        };
        let value = read_json(&path)?;
        serde_json::from_value(value).with_context(|| format!("parse {}", path.display()))
    }
}

impl WindowsVmConfig {
    pub fn expanded_vm_dir(&self) -> PathBuf {
        expand_home(&self.vm_dir)
    }

    pub fn start_command(&self) -> Vec<String> {
        expand_command(&self.start_command, &self.expanded_vm_dir())
    }

    pub fn restart_command(&self) -> Vec<String> {
        expand_command(&self.restart_command, &self.expanded_vm_dir())
    }

    pub fn helper_remote_scp_path(&self) -> String {
        format!(
            "{}:{}",
            self.ssh_host,
            windows_path_for_scp(&self.helper_remote_path)
        )
    }

    pub fn remote_work_dir_ps(&self) -> String {
        ps_single_quoted(&self.remote_work_dir)
    }

    pub fn helper_remote_cmd_path(&self) -> String {
        self.helper_remote_path.clone()
    }
}

fn expand_command(command: &[String], vm_dir: &Path) -> Vec<String> {
    let vm_dir = vm_dir.to_string_lossy();
    command
        .iter()
        .map(|part| part.replace("{vmDir}", &vm_dir))
        .collect()
}

fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}

pub(super) fn windows_path_for_scp(path: &str) -> String {
    path.replace('\\', "/")
}

pub(super) fn ps_single_quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
