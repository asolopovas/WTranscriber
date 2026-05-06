use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{config::Config, error::Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub input: PathBuf,
    pub config: Config,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Utterance {
    pub start_ms: u64,
    pub end_ms: u64,
    pub speaker: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub model: String,
    pub language: String,
    pub duration_ms: u64,
    pub utterances: Vec<Utterance>,
}

pub async fn run(job: &Job) -> Result<Transcript> {
    Ok(Transcript {
        model: job.config.model.clone(),
        language: job.config.language.clone(),
        duration_ms: 0,
        utterances: Vec::new(),
    })
}
