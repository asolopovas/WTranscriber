use std::{
    collections::HashSet,
    sync::{Mutex, OnceLock},
};

use serde::{Deserialize, Serialize};

use crate::{
    error::Result,
    models::{
        catalog::{self, Entry, Family, paths_for},
        download::{Progress, download_file},
    },
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelStatus {
    NotInstalled,
    Downloading,
    Installed,
}

impl ModelStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::Downloading => "downloading",
            Self::NotInstalled => "not_installed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub family: Family,
    pub engine: String,
    pub display_name: String,
    pub description: String,
    pub size_bytes: u64,
    pub default_active: bool,
    pub status: ModelStatus,
    pub languages: Vec<String>,
}

#[derive(Default)]
pub struct Manager {
    in_flight: Mutex<HashSet<String>>,
}

static MANAGER: OnceLock<Manager> = OnceLock::new();

pub fn manager() -> &'static Manager {
    MANAGER.get_or_init(Manager::default)
}

impl Manager {
    pub fn list(&self) -> Result<Vec<ModelInfo>> {
        catalog::catalog().iter().map(|e| self.info(e)).collect()
    }

    #[allow(dead_code)]
    pub fn list_family(&self, family: Family) -> Result<Vec<ModelInfo>> {
        catalog::by_family(family)
            .into_iter()
            .map(|e| self.info(e))
            .collect()
    }

    pub fn status(&self, id: &str) -> Result<ModelStatus> {
        if self.in_flight.lock().unwrap().contains(id) {
            return Ok(ModelStatus::Downloading);
        }
        let Some(entry) = catalog::by_id(id) else {
            return Ok(ModelStatus::NotInstalled);
        };
        for p in paths_for(entry)? {
            if !p.exists() {
                return Ok(ModelStatus::NotInstalled);
            }
        }
        Ok(ModelStatus::Installed)
    }

    fn info(&self, entry: &Entry) -> Result<ModelInfo> {
        Ok(ModelInfo {
            id: entry.id.clone(),
            family: entry.family,
            engine: entry.engine.clone(),
            display_name: entry.display_name.clone(),
            description: entry.description.clone(),
            size_bytes: entry.size_bytes,
            default_active: entry.default_active,
            status: self.status(&entry.id)?,
            languages: entry.languages.clone(),
        })
    }

    pub async fn install(
        &self,
        id: &str,
        on_progress: &mut (dyn FnMut(FileProgress) + Send),
    ) -> Result<()> {
        let entry = catalog::by_id(id)
            .ok_or_else(|| crate::error::Error::Config(format!("unknown model id {id}")))?;

        self.in_flight.lock().unwrap().insert(id.to_owned());
        let result = self.install_inner(entry, on_progress).await;
        self.in_flight.lock().unwrap().remove(id);
        result
    }

    async fn install_inner(
        &self,
        entry: &Entry,
        on_progress: &mut (dyn FnMut(FileProgress) + Send),
    ) -> Result<()> {
        let dests = paths_for(entry)?;
        for (i, (file, dst)) in entry.files.iter().zip(dests).enumerate() {
            if dst.exists() {
                on_progress(FileProgress {
                    id: entry.id.clone(),
                    file_index: i,
                    file_count: entry.files.len(),
                    rel_path: file.rel_path.clone(),
                    downloaded: file.size_bytes,
                    total: file.size_bytes,
                });
                continue;
            }
            let id = entry.id.clone();
            let rel = file.rel_path.clone();
            let count = entry.files.len();
            let mut cb = |p: Progress| {
                on_progress(FileProgress {
                    id: id.clone(),
                    file_index: i,
                    file_count: count,
                    rel_path: rel.clone(),
                    downloaded: p.downloaded,
                    total: p.total,
                });
            };
            let sha = (!file.sha256.is_empty()).then_some(file.sha256.as_str());
            download_file(&dst, &file.url, sha, &mut cb).await?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FileProgress {
    pub id: String,
    pub file_index: usize,
    pub file_count: usize,
    pub rel_path: String,
    pub downloaded: u64,
    pub total: u64,
}

#[allow(dead_code)]
pub fn list() -> Result<Vec<ModelInfo>> {
    manager().list()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_strings_use_snake_case() {
        assert_eq!(ModelStatus::Installed.as_str(), "installed");
        assert_eq!(ModelStatus::Downloading.as_str(), "downloading");
        assert_eq!(ModelStatus::NotInstalled.as_str(), "not_installed");
    }

    #[test]
    fn status_serialises_to_snake_case() {
        let raw = serde_json::to_string(&ModelStatus::NotInstalled).unwrap();
        assert_eq!(raw, "\"not_installed\"");
    }
}
