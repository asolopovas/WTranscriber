use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{
    audio,
    config::Config,
    error::Result,
    transcriber::transcript::{self, Meta, Segment, Transcript},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub input: PathBuf,
    pub config: Config,
}

pub async fn run(job: &Job) -> Result<Transcript> {
    let input = job.input.clone();
    let config = job.config.clone();

    let (samples, duration_ms) = tokio::task::spawn_blocking(move || -> Result<(Vec<f32>, u64)> {
        let s = audio::load_samples(&input)?;
        let dur = (s.len() as u64 * 1000) / u64::from(audio::WHISPER_SAMPLE_RATE);
        Ok((s, dur))
    })
    .await
    .map_err(|e| crate::error::Error::Transcribe(format!("audio task: {e}")))??;

    let _ = samples;

    let segments: Vec<Segment> = Vec::new();
    let diar = Vec::new();

    Ok(transcript::build(
        &segments,
        &diar,
        Meta {
            model: config.model,
            language: config.language,
            duration_ms,
            diarizer: None,
            device: Some(format!("{:?}", config.device).to_lowercase()),
        },
    ))
}
