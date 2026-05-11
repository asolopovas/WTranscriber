#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use std::sync::Arc;

use std::sync::atomic::{AtomicBool, Ordering};

use crate::{
    audio,
    audio_toolkit::{
        stream::stream_slabs,
        vad::{self, RegionStream, RegionStreamConfig},
    },
    config::{Config, Engine},
    diarizer::{self, Segment as DiarSegment},
    engine,
    error::Result,
    logfile,
    progress::{self, NoopSink, Phase, Sink},
    transcriber::{
        cache::{self, Entry as CacheEntry, build_key_params, compute_key},
        dedup,
        format::format_hms,
        partial,
        transcript::{self, Meta, Segment, Transcript},
    },
};

const DEFAULT_SLAB_SEC: f64 = 60.0;
const EPSILON_SEC: f64 = 1e-3;

fn slab_sec() -> f64 {
    std::env::var("WT_SLAB_SEC")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|v| *v > 0.0)
        .unwrap_or(DEFAULT_SLAB_SEC)
}

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

    if matches!(config.engine, Engine::WhisperOnnx) && !vad::model::is_installed() {
        match vad::model::ensure().await {
            Ok(p) => logfile::info(&format!("silero vad ready: {}", p.display())),
            Err(e) => logfile::warn(&format!(
                "silero vad fetch failed ({e}); falling back to fixed slabs"
            )),
        }
    }

    tokio::task::spawn_blocking(move || run_blocking(&input, &config, sink.as_ref()))
        .await
        .map_err(|e| crate::error::Error::Transcribe(format!("task: {e}")))?
}

#[allow(clippy::too_many_lines)]
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

    // Fail fast: if the configured engine needs a subprocess binary that's
    // missing, surface the error before we open ffmpeg / probe duration.
    engine::preflight(config)?;

    if let Some(cached) = cache::load(&key)? {
        let stale = config.diarize && cached.speakers_detected == 0;
        if stale {
            logfile::info(&format!(
                "cache hit but stale (0 speakers with diarize on); rerunning key={key}"
            ));
            let _ = cache::invalidate(&key);
        } else {
            logfile::info(&format!(
                "cache hit; reusing transcript ({} utterances, {}, {} speakers)",
                cached.utterances.len(),
                format_hms(std::time::Duration::from_millis(cached.duration_ms)),
                cached.speakers_detected,
            ));
            sink.phase(Phase::Done);
            return Ok(cached);
        }
    }

    if sink.is_cancelled() {
        return Err(crate::error::Error::Cancelled);
    }

    sink.phase(Phase::LoadingAudio);
    let probe_t0 = std::time::Instant::now();
    let total_dur_ms = audio::probe_duration_ms(input).unwrap_or(0);
    logfile::info(&format!(
        "audio probe: {} dur={} (probed in {:.2}s)",
        input.display(),
        format_hms(std::time::Duration::from_millis(total_dur_ms)),
        probe_t0.elapsed().as_secs_f64(),
    ));
    let start_ms = trim.trim_start_ms.min(total_dur_ms.max(trim.trim_start_ms));
    let end_ms_opt = trim.trim_end_ms.map(|e| {
        if total_dur_ms > 0 {
            e.min(total_dur_ms)
        } else {
            e
        }
    });
    let trimmed_dur_ms = end_ms_opt
        .map(|e| e.saturating_sub(start_ms))
        .or_else(|| (total_dur_ms > 0).then(|| total_dur_ms.saturating_sub(start_ms)));
    let trimmed_dur_sec = trimmed_dur_ms.map_or(0.0, |ms| ms as f64 / 1000.0);
    if start_ms > 0 || end_ms_opt.is_some() {
        logfile::info(&format!(
            "trim active: {}-{} (slice {})",
            format_hms(std::time::Duration::from_millis(start_ms)),
            end_ms_opt.map_or_else(
                || "end".into(),
                |e| format_hms(std::time::Duration::from_millis(e)),
            ),
            format_hms(std::time::Duration::from_secs_f64(trimmed_dur_sec)),
        ));
    }

    let mut state = partial::load(&key).unwrap_or_else(|| partial::Partial {
        key: key.clone(),
        last_done_sec: 0.0,
        segments: Vec::new(),
    });
    let slab = slab_sec();
    let use_vad = matches!(config.engine, Engine::WhisperOnnx) && vad::model::is_installed();
    let vad_max_region_sec: f64 = 15.0;
    if state.segments.is_empty() {
        if use_vad {
            logfile::info(&format!(
                "streaming start: vad-regions max={vad_max_region_sec:.0}s engine={} model={}",
                config.engine.as_str(),
                config.model,
            ));
        } else {
            logfile::info(&format!(
                "streaming start: slab={slab:.0}s engine={} model={}",
                config.engine.as_str(),
                config.model,
            ));
        }
    } else {
        logfile::info(&format!(
            "resuming from {} ({} cached segs)",
            format_hms(std::time::Duration::from_secs_f64(state.last_done_sec)),
            state.segments.len(),
        ));
    }

    sink.phase(Phase::Transcribing);
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_for_stream = cancel_flag.clone();
    let mut total_audio = 0.0_f64;
    let mut total_elapsed = 0.0_f64;
    let mut detected_language = String::new();
    let mut slab_index: usize = 0;
    let pipeline_t0 = std::time::Instant::now();

    let mut process_region = |region: crate::audio_toolkit::vad::Region| -> Result<()> {
        if sink.is_cancelled() {
            cancel_flag.store(true, Ordering::SeqCst);
            return Err(crate::error::Error::Cancelled);
        }
        if region.end_sec <= state.last_done_sec + EPSILON_SEC {
            emit_pct(sink, region.end_sec, trimmed_dur_sec);
            return Ok(());
        }
        slab_index += 1;
        let mut sub_progress = |_pct: f64| {};
        let cancelled = || sink.is_cancelled();
        let region_dur_sec = region.end_sec - region.start_sec;
        let region_start_sec = region.start_sec;
        let region_end_sec = region.end_sec;
        let resume_floor = state.last_done_sec;
        let t0 = std::time::Instant::now();
        let mut slab_segs_emitted: usize = 0;
        let mut slab_dropped: usize = 0;
        let mut save_err: Option<crate::error::Error> = None;
        let mut on_chunk = |mut segs: Vec<Segment>, chunk_end_sec: f64| {
            let abs_end = (region_start_sec + chunk_end_sec).min(region_end_sec);
            if abs_end <= resume_floor + EPSILON_SEC {
                return;
            }
            let before = segs.len();
            apply_dedup(&mut segs);
            slab_dropped += before.saturating_sub(segs.len());
            shift_segments(&mut segs, (region_start_sec * 1000.0) as u64);
            state.segments.extend(segs.iter().cloned());
            slab_segs_emitted += segs.len();
            state.last_done_sec = abs_end;
            if save_err.is_none() {
                if let Err(e) = partial::save(&state) {
                    save_err = Some(e);
                }
            }
            emit_pct(sink, abs_end, trimmed_dur_sec);
        };
        let engine_result = engine::run(
            &region.samples,
            region_dur_sec,
            config,
            &mut sub_progress,
            &cancelled,
            &mut on_chunk,
        );
        let (slab_detected, _rtf) = engine_result?;
        if let Some(e) = save_err {
            return Err(e);
        }
        if detected_language.is_empty() && !slab_detected.is_empty() {
            detected_language.clone_from(&slab_detected);
            logfile::info(&format!("detected language: {slab_detected}"));
        }
        let elapsed = t0.elapsed().as_secs_f64();
        total_audio += region_dur_sec;
        total_elapsed += elapsed;
        let slab_rtf = if elapsed > 0.0 {
            region_dur_sec / elapsed
        } else {
            0.0
        };
        logfile::info(&format!(
            "slab #{slab_index} {}-{} ({:.1}s in {:.1}s rtf={:.2}, {} segs{})",
            format_hms(std::time::Duration::from_secs_f64(region.start_sec)),
            format_hms(std::time::Duration::from_secs_f64(region.end_sec)),
            region_dur_sec,
            elapsed,
            slab_rtf,
            slab_segs_emitted,
            if slab_dropped > 0 {
                format!(", dropped {slab_dropped} dedup")
            } else {
                String::new()
            },
        ));
        Ok(())
    };

    let stream_result = if use_vad {
        let model = vad::model::model_path()?;
        let vad_cfg = RegionStreamConfig {
            max_region_sec: vad_max_region_sec,
            ..RegionStreamConfig::default()
        };
        RegionStream::run(
            input,
            start_ms,
            end_ms_opt,
            &model,
            vad_cfg,
            cancel_for_stream,
            &mut process_region,
        )
    } else {
        stream_slabs(
            input,
            start_ms,
            end_ms_opt,
            slab,
            cancel_for_stream,
            |region| process_region(region).map(|()| true),
        )
    };

    let scanned_end = stream_result?;
    if sink.is_cancelled() {
        return Err(crate::error::Error::Cancelled);
    }

    let observed_rtf = if total_elapsed > 0.0 {
        total_audio / total_elapsed
    } else {
        0.0
    };
    let device_label = config.device.as_str().to_owned();
    if observed_rtf > 0.0 {
        progress::save_rtf(&config.model, &device_label, observed_rtf);
    }
    logfile::info(&format!(
        "transcribed: {} slab(s) {:.1}s audio in {:.1}s rtf={:.2} (wall {:.1}s)",
        slab_index,
        total_audio,
        total_elapsed,
        observed_rtf,
        pipeline_t0.elapsed().as_secs_f64(),
    ));

    let mut segments = state.segments.clone();
    apply_dedup(&mut segments);

    if detected_language.is_empty() {
        detected_language.clone_from(&config.language);
    }
    let duration_ms = if total_dur_ms > 0 {
        total_dur_ms
    } else {
        ((scanned_end + start_ms as f64 / 1000.0) * 1000.0) as u64
    };
    let _ = start_ms;

    let (diar_segs, diar_name) = if config.diarize {
        if sink.is_cancelled() {
            return Err(crate::error::Error::Cancelled);
        }
        sink.phase(Phase::Diarizing);
        let diar_dur_sec = if total_dur_ms > 0 {
            total_dur_ms as f64 / 1000.0
        } else {
            scanned_end
        };
        let diar_t0 = std::time::Instant::now();
        logfile::info(&format!(
            "diarize start: backend={} speakers={}",
            config.diarizer.as_str(),
            speakers,
        ));
        let result = run_diarize_streaming(input, diar_dur_sec, speakers, sink, config);
        match result {
            Ok((segs, name)) => {
                let unique = segs
                    .iter()
                    .map(|s| s.speaker)
                    .collect::<std::collections::HashSet<_>>()
                    .len();
                logfile::info(&format!(
                    "diarized: {name} · {unique} speakers · {} segments · {:.1}s",
                    segs.len(),
                    diar_t0.elapsed().as_secs_f64(),
                ));
                (segs, Some(name))
            }
            Err(e) => {
                logfile::warn(&format!("diarization failed: {e}"));
                (Vec::new(), None)
            }
        }
    } else {
        (Vec::new(), None)
    };
    if sink.is_cancelled() {
        return Err(crate::error::Error::Cancelled);
    }

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
        key: key.clone(),
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
    let _ = partial::clear(&key);
    logfile::info(&format!(
        "cache stored: key={} utterances={} speakers_detected={}",
        key,
        transcript.utterances.len(),
        transcript.speakers_detected,
    ));
    sink.phase(Phase::Done);
    Ok(transcript)
}

fn emit_pct(sink: &dyn Sink, done_sec: f64, total_sec: f64) {
    if total_sec <= 0.0 {
        return;
    }
    let pct = (done_sec / total_sec * 100.0).clamp(0.0, 99.9);
    sink.report_pct(Phase::Transcribing, pct);
}

fn shift_segments(segments: &mut [Segment], offset_ms: u64) {
    if offset_ms == 0 {
        return;
    }
    for seg in segments.iter_mut() {
        seg.start_ms = seg.start_ms.saturating_add(offset_ms);
        seg.end_ms = seg.end_ms.saturating_add(offset_ms);
        for tok in &mut seg.tokens {
            tok.start_ms = tok.start_ms.saturating_add(offset_ms);
            tok.end_ms = tok.end_ms.saturating_add(offset_ms);
        }
    }
}

fn run_diarize_streaming(
    input: &Path,
    audio_dur_sec: f64,
    speakers: u32,
    sink: &dyn Sink,
    config: &Config,
) -> Result<(Vec<DiarSegment>, String)> {
    let wav = audio::ensure_cached_wav(input)?;
    let backend = diarizer::new_with_choice(speakers, config.diarizer)?;
    let backend_name = backend.name();
    sink.set_diarize_backend(&backend_name);
    let mut on_progress = |pct: f64| sink.report_pct(Phase::Diarizing, pct);
    let cancelled = || sink.is_cancelled();
    match backend.diarize(&wav, speakers, audio_dur_sec, &cancelled, &mut on_progress) {
        Ok(segs) => Ok((segs, backend_name)),
        Err(e)
            if config.diarizer == crate::config::DiarizerChoice::Nemo
                && backend_name == "nemo-sortformer" =>
        {
            logfile::warn(&format!(
                "diarizer nemo failed at runtime ({e}); falling back to titanet"
            ));
            let fallback =
                diarizer::new_with_choice(speakers, crate::config::DiarizerChoice::Titanet)?;
            let fallback_name = fallback.name();
            sink.set_diarize_backend(&fallback_name);
            let mut fb_progress = |pct: f64| sink.report_pct(Phase::Diarizing, pct);
            let fb_cancelled = || sink.is_cancelled();
            let segs = fallback.diarize(
                &wav,
                speakers,
                audio_dur_sec,
                &fb_cancelled,
                &mut fb_progress,
            )?;
            Ok((segs, fallback_name))
        }
        Err(e) => Err(e),
    }
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

#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use super::*;
    use crate::transcriber::transcript::Token;
    use std::sync::Mutex;

    // Serialise env-var mutations across parallel test threads.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn set_env(key: &str, value: &str) {
        // SAFETY: caller holds ENV_LOCK.
        unsafe {
            std::env::set_var(key, value);
        }
    }

    fn unset_env(key: &str) {
        // SAFETY: see `set_env`.
        unsafe {
            std::env::remove_var(key);
        }
    }

    fn tok(text: &str, start: u64, end: u64) -> Token {
        Token {
            text: text.into(),
            start_ms: start,
            end_ms: end,
            confidence: 1.0,
        }
    }

    fn seg(text: &str, start: u64, end: u64, tokens: Vec<Token>) -> Segment {
        Segment {
            text: text.into(),
            start_ms: start,
            end_ms: end,
            tokens,
        }
    }

    #[test]
    fn slab_sec_env_handling() {
        let _g = ENV_LOCK.lock().unwrap();

        unset_env("WT_SLAB_SEC");
        assert!((slab_sec() - DEFAULT_SLAB_SEC).abs() < f64::EPSILON);

        for invalid in ["0", "-5", "not-a-number"] {
            set_env("WT_SLAB_SEC", invalid);
            assert!(
                (slab_sec() - DEFAULT_SLAB_SEC).abs() < f64::EPSILON,
                "input {invalid:?} should fall back to default"
            );
        }

        set_env("WT_SLAB_SEC", "30");
        assert!((slab_sec() - 30.0).abs() < f64::EPSILON);

        unset_env("WT_SLAB_SEC");
    }

    #[test]
    fn shift_segments_no_op_when_offset_zero() {
        let mut segs = vec![seg("hi", 100, 200, vec![tok("hi", 100, 200)])];
        let before = segs.clone();
        shift_segments(&mut segs, 0);
        assert_eq!(segs[0].start_ms, before[0].start_ms);
        assert_eq!(segs[0].end_ms, before[0].end_ms);
        assert_eq!(segs[0].tokens[0].start_ms, before[0].tokens[0].start_ms);
    }

    #[test]
    fn shift_segments_adds_offset_to_segments_and_tokens() {
        let mut segs = vec![seg("x", 10, 20, vec![tok("x", 10, 15), tok("y", 16, 20)])];
        shift_segments(&mut segs, 1_000);
        assert_eq!(segs[0].start_ms, 1_010);
        assert_eq!(segs[0].end_ms, 1_020);
        assert_eq!(segs[0].tokens[0].start_ms, 1_010);
        assert_eq!(segs[0].tokens[1].end_ms, 1_020);
    }

    #[test]
    fn shift_segments_saturates_at_u64_max() {
        let mut segs = vec![seg("x", u64::MAX - 5, u64::MAX - 1, vec![])];
        shift_segments(&mut segs, 1_000);
        assert_eq!(segs[0].start_ms, u64::MAX);
        assert_eq!(segs[0].end_ms, u64::MAX);
    }

    #[test]
    fn rebuild_from_tokens_clears_when_empty() {
        let mut s = seg("stale", 100, 200, vec![]);
        rebuild_from_tokens(&mut s);
        assert!(s.text.is_empty());
    }

    #[test]
    fn rebuild_from_tokens_recomputes_bounds_and_text() {
        let mut s = seg(
            "old",
            999,
            999,
            vec![tok("hello", 10, 20), tok("world", 21, 30)],
        );
        rebuild_from_tokens(&mut s);
        assert_eq!(s.text, "hello world");
        assert_eq!(s.start_ms, 10);
        assert_eq!(s.end_ms, 30);
    }

    #[test]
    fn apply_dedup_removes_empty_segments() {
        let mut segs = vec![
            seg("", 0, 0, vec![]),
            seg("ok", 10, 20, vec![tok("ok", 10, 20)]),
            seg("   ", 30, 40, vec![]),
        ];
        apply_dedup(&mut segs);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "ok");
    }

    #[test]
    fn apply_dedup_collapses_token_repeats_and_rebuilds() {
        // dedup needs >= 4 unigram repeats to collapse (see MIN_RUN_BY_N1).
        let mut segs = vec![seg(
            "the the the the",
            100,
            500,
            vec![
                tok("the", 100, 200),
                tok("the", 200, 300),
                tok("the", 300, 400),
                tok("the", 400, 500),
            ],
        )];
        apply_dedup(&mut segs);
        assert_eq!(segs.len(), 1);
        // After collapse + rebuild_from_tokens, text matches the surviving tokens.
        assert_eq!(segs[0].tokens.len(), 1);
        assert_eq!(segs[0].text, "the");
        assert_eq!(segs[0].start_ms, 100);
        assert_eq!(segs[0].end_ms, 200);
    }

    #[test]
    fn apply_dedup_collapses_in_plain_text_when_no_tokens() {
        // No tokens path — collapse_in_text trims and dedups.
        let mut segs = vec![seg("hello hello hello hello world", 0, 100, vec![])];
        apply_dedup(&mut segs);
        assert_eq!(segs.len(), 1);
        assert!(
            !segs[0].text.contains("hello hello"),
            "dedup should leave at most one 'hello' run, got {:?}",
            segs[0].text
        );
    }
}
