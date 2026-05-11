mod catalog;
mod catalog_data;
pub mod download;
mod manager;

pub use catalog::{Family, by_family, by_id, default_id, model_dir, paths_for};
pub use manager::{FileProgress, ModelInfo, ModelStatus, manager};
