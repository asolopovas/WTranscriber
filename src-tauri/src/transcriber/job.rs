#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{
    audio,
    config::Config,
    engine,
    error::Result,
    transcriber::transcript::{self, Meta, Transcript},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub input: PathBuf,
    pub config: Config,
}

pub async fn run(job: &Job) -> Result<Transcript> {
    let input = job.input.clone();
    let config = job.config.clone();

    tokio::task::spawn_blocking(move || run_blocking(&input, &config))
        .await
        .map_err(|e| crate::error::Error::Transcribe(format!("task: {e}")))?
}

fn run_blocking(input: &std::path::Path, config: &Config) -> Result<Transcript> {
    let samples = audio::load_samples(input)?;
    let audio_dur_sec = samples.len() as f64 / f64::from(audio::WHISPER_SAMPLE_RATE);
    let duration_ms = (audio_dur_sec * 1000.0) as u64;

    let mut on_progress = |_pct: f64| {};
    let (segments, detected_language, _rtf) =
        engine::run_whisper(&samples, audio_dur_sec, config, &mut on_progress)?;

    let diar = Vec::new();
    Ok(transcript::build(
        &segments,
        &diar,
        Meta {
            model: config.model.clone(),
            language: if detected_language.is_empty() {
                config.language.clone()
            } else {
                detected_language
            },
            duration_ms,
            diarizer: None,
            device: Some(format!("{:?}", config.device).to_lowercase()),
        },
    ))
}
