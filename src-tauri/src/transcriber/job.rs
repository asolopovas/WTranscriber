#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

mod diarize;
mod finalize;
mod postprocess;
mod slab;
mod streaming;
mod trim;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::{Deserialize, Serialize};

use crate::{
    audio,
    config::{Config, Engine},
    engine,
    error::Result,
    logfile,
    progress::{NoopSink, Phase, Sink},
    transcriber::{
        cache::{self, KeyOptions, build_key_params, compute_key},
        format::format_hms,
        transcript::Transcript,
    },
};

use self::{
    diarize::run_diarize_phase,
    finalize::{BuildArgs, build_and_cache_transcript},
    postprocess::apply_dedup,
    streaming::run_streaming_phase,
    trim::compute_trim_window,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub input: PathBuf,
    pub config: Config,
}

struct EngineShutdown;

impl Drop for EngineShutdown {
    fn drop(&mut self) {
        engine::shutdown();
    }
}

pub async fn run(job: &Job) -> Result<Transcript> {
    run_with_sink(job, Arc::new(NoopSink)).await
}

pub async fn run_with_sink(job: &Job, sink: Arc<dyn Sink>) -> Result<Transcript> {
    let input = job.input.clone();
    let config = job.config.clone();

    tokio::task::spawn_blocking(move || run_blocking(&input, &config, sink.as_ref()))
        .await
        .map_err(|e| crate::error::Error::Transcribe(format!("task: {e}")))?
}

fn try_serve_from_cache(key: &str, config: &Config, sink: &dyn Sink) -> Result<Option<Transcript>> {
    let Some(cached) = cache::load(key)? else {
        return Ok(None);
    };
    if config.diarize && cached.speakers_detected == 0 {
        logfile::info(&format!(
            "cache hit but stale (0 speakers with diarize on); rerunning key={key}"
        ));
        let _ = cache::invalidate(key);
        return Ok(None);
    }
    logfile::info(&format!(
        "cache hit; reusing transcript ({} utterances, {}, {} speakers)",
        cached.utterances.len(),
        format_hms(std::time::Duration::from_millis(cached.duration_ms)),
        cached.speakers_detected,
    ));
    sink.phase(Phase::Done);
    Ok(Some(cached))
}

fn run_blocking(input: &Path, config: &Config, sink: &dyn Sink) -> Result<Transcript> {
    sink.phase(Phase::CacheCheck);
    let speakers = config.speakers.unwrap_or(0);
    let trim = audio::meta::load(input).unwrap_or_default();
    let key_params = build_key_params(
        input,
        KeyOptions {
            model: &config.model,
            language: &config.language,
            speakers,
            no_diarize: !config.diarize,
            trim_start_ms: trim.trim_start_ms,
            trim_end_ms: trim.trim_end_ms.unwrap_or(0),
            precise_word_timestamps: matches!(config.engine, Engine::WhisperCpp)
                && config.precise_word_timestamps,
        },
    )?;
    let key = compute_key(&key_params);

    engine::preflight(config)?;
    let _engine_guard = EngineShutdown;

    if let Some(cached) = try_serve_from_cache(&key, config, sink)? {
        return Ok(cached);
    }
    if sink.is_cancelled() {
        return Err(crate::error::Error::Cancelled);
    }

    sink.phase(Phase::LoadingAudio);
    let window = compute_trim_window(input, &trim);
    let device_label = config.device.as_str().to_owned();
    let (st, scanned_end) = run_streaming_phase(input, config, sink, &key, &window)?;

    let mut segments = st.state.segments.clone();
    apply_dedup(&mut segments);

    let duration_ms = if window.total_dur_ms > 0 {
        window.total_dur_ms
    } else {
        (scanned_end * 1000.0) as u64
    };

    let (diar_segs, diar_name) = if config.diarize {
        if sink.is_cancelled() {
            return Err(crate::error::Error::Cancelled);
        }
        run_diarize_phase(
            input,
            sink,
            config,
            speakers,
            window.total_dur_ms,
            scanned_end,
        )
    } else {
        (Vec::new(), None)
    };
    if sink.is_cancelled() {
        return Err(crate::error::Error::Cancelled);
    }

    sink.phase(Phase::Writing);
    let language = if st.detected_language.is_empty() {
        config.language.clone()
    } else {
        st.detected_language
    };
    let transcript = build_and_cache_transcript(BuildArgs {
        segments: &segments,
        diar_segs: &diar_segs,
        diar_name,
        config,
        key: &key,
        key_params: &key_params,
        speakers,
        language,
        device_label,
        duration_ms,
    })?;
    sink.phase(Phase::Done);
    Ok(transcript)
}
