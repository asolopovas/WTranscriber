use std::path::PathBuf;

use crate::{
    error::Result,
    models::download::{Progress, download_file},
    paths,
};

const SILERO_URL: &str =
    "https://raw.githubusercontent.com/snakers4/silero-vad/v4.0/files/silero_vad.onnx";
const SILERO_SHA: &str = "a35ebf52fd3ce5f1469b2a36158dba761bc47b973ea3382b3186ca15b1f5af28";
const REL_PATH: &str = "aux/silero_vad.onnx";

pub fn model_path() -> Result<PathBuf> {
    Ok(paths::models_dir()?.join(REL_PATH))
}

pub fn is_installed() -> bool {
    model_path().is_ok_and(|p| p.exists())
}

pub async fn ensure() -> Result<PathBuf> {
    let dst = model_path()?;
    if dst.exists() {
        return Ok(dst);
    }
    let mut cb = |_: Progress| {};
    download_file(&dst, SILERO_URL, Some(SILERO_SHA), &mut cb).await?;
    Ok(dst)
}

#[allow(dead_code)]
pub const fn expected_sha() -> &'static str {
    SILERO_SHA
}
