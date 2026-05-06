#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use std::sync::Arc;

use crate::{
    audio,
    config::Config,
    diarizer::{self, Segment as DiarSegment},
    engine,
    error::Result,
    progress::{self, NoopSink, Phase, Sink},
    transcriber::{
        cache::{self, Entry as CacheEntry, build_key_params, compute_key},
        dedup,
        transcript::{self, Meta, Segment, Transcript},
    },
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub input: PathBuf,
    pub config: Config,
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

fn run_blocking(input: &Path, config: &Config, sink: &dyn Sink) -> Result<Transcript> {
    sink.phase(Phase::CacheCheck);
    let speakers = config.speakers.unwrap_or(0);
    let trim = audio::meta::load(input).unwrap_or_default();
    let key_params = build_key_params(
        input,
        &config.model,
        &config.language,
        speakers,
        !config.diarize,
        trim.trim_start_ms,
        trim.trim_end_ms.unwrap_or(0),
    )?;
    let key = compute_key(&key_params);

    if let Some(cached) = cache::load(&key)? {
        sink.phase(Phase::Done);
        return Ok(cached);
    }

    if sink.is_cancelled() {
        return Err(crate::error::Error::Cancelled);
    }

    sink.phase(Phase::LoadingAudio);
    let all_samples = audio::load_samples(input)?;
    if sink.is_cancelled() {
        return Err(crate::error::Error::Cancelled);
    }
    let sr = f64::from(audio::WHISPER_SAMPLE_RATE);
    let total_dur_ms = (all_samples.len() as f64 / sr * 1000.0) as u64;
    let start_ms = trim.trim_start_ms.min(total_dur_ms);
    let end_ms = trim
        .trim_end_ms
        .map_or(total_dur_ms, |e| e.min(total_dur_ms))
        .max(start_ms);
    let start_idx = ((start_ms as f64 / 1000.0) * sr) as usize;
    let end_idx = ((end_ms as f64 / 1000.0) * sr) as usize;
    let samples: Vec<f32> = if start_idx == 0 && end_idx >= all_samples.len() {
        all_samples
    } else {
        all_samples[start_idx.min(all_samples.len())..end_idx.min(all_samples.len())].to_vec()
    };
    let audio_dur_sec = samples.len() as f64 / sr;
    let duration_ms = total_dur_ms;
    let offset_ms = start_ms;

    sink.phase(Phase::Transcribing);
    let mut on_progress = |pct: f64| {
        sink.report_pct(Phase::Transcribing, pct);
    };
    let cancelled = || sink.is_cancelled();
    let (mut segments, detected_language, observed_rtf) = engine::run(
        &samples,
        audio_dur_sec,
        config,
        &mut on_progress,
        &cancelled,
    )?;
    apply_dedup(&mut segments);
    if offset_ms > 0 {
        for seg in &mut segments {
            seg.start_ms = seg.start_ms.saturating_add(offset_ms);
            seg.end_ms = seg.end_ms.saturating_add(offset_ms);
            for tok in &mut seg.tokens {
                tok.start_ms = tok.start_ms.saturating_add(offset_ms);
                tok.end_ms = tok.end_ms.saturating_add(offset_ms);
            }
        }
    }

    if sink.is_cancelled() {
        return Err(crate::error::Error::Cancelled);
    }

    let device_label = format!("{:?}", config.device).to_lowercase();
    progress::save_rtf(&config.model, &device_label, observed_rtf);

    let (diar_segs, diar_name) = if config.diarize {
        if sink.is_cancelled() {
            return Err(crate::error::Error::Cancelled);
        }
        sink.phase(Phase::Diarizing);
        let result = run_diarize(input, &samples, audio_dur_sec, speakers, sink).map_or(
            (Vec::new(), None),
            |(mut segs, n)| {
                if offset_ms > 0 {
                    let off = offset_ms as f64 / 1000.0;
                    for s in &mut segs {
                        s.start_sec += off;
                        s.end_sec += off;
                    }
                }
                (segs, Some(n))
            },
        );
        if sink.is_cancelled() {
            return Err(crate::error::Error::Cancelled);
        }
        result
    } else {
        (Vec::new(), None)
    };

    sink.phase(Phase::Writing);

    let language = if detected_language.is_empty() {
        config.language.clone()
    } else {
        detected_language
    };

    let transcript = transcript::build(
        &segments,
        &diar_segs,
        Meta {
            model: config.model.clone(),
            language: language.clone(),
            duration_ms,
            diarizer: diar_name,
            device: Some(device_label),
        },
    );

    let entry = CacheEntry {
        key,
        source_path: key_params.source_path.clone(),
        source_name: key_params
            .source_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        model: config.model.clone(),
        language,
        speakers,
        no_diarize: !config.diarize,
        utterances: transcript.utterances.len(),
        duration_ms,
        created_at: Utc::now(),
        size_bytes: 0,
    };
    cache::store(entry, &transcript)?;
    sink.phase(Phase::Done);
    Ok(transcript)
}

fn apply_dedup(segments: &mut Vec<Segment>) {
    for seg in segments.iter_mut() {
        if seg.tokens.len() >= 2 {
            let collapsed = dedup::collapse_repeats(&seg.tokens);
            if collapsed.len() != seg.tokens.len() {
                seg.tokens = collapsed;
                rebuild_from_tokens(seg);
            }
        } else if !seg.text.trim().is_empty() {
            seg.text = dedup::collapse_in_text(seg.text.trim());
        }
    }
    segments.retain(|s| !s.tokens.is_empty() || !s.text.trim().is_empty());
}

fn rebuild_from_tokens(seg: &mut Segment) {
    if seg.tokens.is_empty() {
        seg.text.clear();
        return;
    }
    seg.text = seg
        .tokens
        .iter()
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    seg.start_ms = seg.tokens.first().unwrap().start_ms;
    seg.end_ms = seg.tokens.last().unwrap().end_ms;
}

fn run_diarize(
    input: &Path,
    samples: &[f32],
    audio_dur_sec: f64,
    speakers: u32,
    sink: &dyn Sink,
) -> Result<(Vec<DiarSegment>, String)> {
    let backend = diarizer::new(speakers)?;
    let wav = ensure_wav_for_diarize(input, samples)?;
    let mut on_progress = |pct: f64| sink.report_pct(Phase::Diarizing, pct);
    let cancelled = || sink.is_cancelled();
    let segs = backend.diarize(&wav, speakers, audio_dur_sec, &cancelled, &mut on_progress)?;
    Ok((segs, backend.name()))
}

fn ensure_wav_for_diarize(input: &Path, samples: &[f32]) -> Result<PathBuf> {
    if input
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
    {
        return Ok(input.to_path_buf());
    }
    let cache_dir = crate::paths::cache_dir()?;
    let key = audio::audio_cache_key(input)?;
    let cached = cache_dir.join(key);
    if cached.exists() {
        return Ok(cached);
    }
    audio::write_pcm16_wav(&cached, samples, audio::WHISPER_SAMPLE_RATE)?;
    Ok(cached)
}
