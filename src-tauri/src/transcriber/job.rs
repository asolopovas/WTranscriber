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
    audio_toolkit::stream::stream_slabs,
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
const ANDROID_WHISPER_CPP_SLAB_SEC: f64 = 15.0;
const EPSILON_SEC: f64 = 1e-3;

fn default_slab_sec(config: &Config) -> f64 {
    if cfg!(target_os = "android") && matches!(config.engine, Engine::WhisperCpp) {
        ANDROID_WHISPER_CPP_SLAB_SEC
    } else {
        DEFAULT_SLAB_SEC
    }
}

fn slab_sec(config: &Config) -> f64 {
    std::env::var("WT_SLAB_SEC")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|v| *v > 0.0)
        .unwrap_or_else(|| default_slab_sec(config))
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

    tokio::task::spawn_blocking(move || run_blocking(&input, &config, sink.as_ref()))
        .await
        .map_err(|e| crate::error::Error::Transcribe(format!("task: {e}")))?
}

struct TrimWindow {
    start_ms: u64,
    end_ms_opt: Option<u64>,
    trimmed_dur_sec: f64,
    total_dur_ms: u64,
}

fn compute_trim_window(input: &Path, trim: &audio::meta::AudioMeta) -> TrimWindow {
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
    TrimWindow {
        start_ms,
        end_ms_opt,
        trimmed_dur_sec,
        total_dur_ms,
    }
}

struct StreamCtx<'a> {
    sink: &'a dyn Sink,
    config: &'a Config,
    trimmed_dur_sec: f64,
}

struct StreamState {
    state: partial::Partial,
    slab_index: usize,
    total_audio: f64,
    total_elapsed: f64,
    detected_language: String,
}

struct ChunkAcc<'a> {
    state: &'a mut partial::Partial,
    region_start_sec: f64,
    region_end_sec: f64,
    resume_floor: f64,
    trimmed_dur_sec: f64,
    sink: &'a dyn Sink,
    segs_emitted: usize,
    dropped: usize,
    save_err: Option<crate::error::Error>,
}

impl ChunkAcc<'_> {
    fn on_chunk(&mut self, mut segs: Vec<Segment>, chunk_end_sec: f64) {
        let abs_end = (self.region_start_sec + chunk_end_sec).min(self.region_end_sec);
        if abs_end <= self.resume_floor + EPSILON_SEC {
            return;
        }
        let before = segs.len();
        apply_dedup(&mut segs);
        self.dropped += before.saturating_sub(segs.len());
        shift_segments(&mut segs, (self.region_start_sec * 1000.0) as u64);
        self.state.segments.extend(segs.iter().cloned());
        self.segs_emitted += segs.len();
        self.state.last_done_sec = abs_end;
        if self.save_err.is_none()
            && let Err(e) = partial::save(self.state)
        {
            self.save_err = Some(e);
        }
        emit_pct(self.sink, abs_end, self.trimmed_dur_sec);
    }
}

fn process_region(
    ctx: &StreamCtx<'_>,
    st: &mut StreamState,
    region: &crate::audio_toolkit::vad::Region,
    cancel_flag: &Arc<AtomicBool>,
) -> Result<()> {
    if ctx.sink.is_cancelled() {
        cancel_flag.store(true, Ordering::SeqCst);
        return Err(crate::error::Error::Cancelled);
    }
    if region.end_sec <= st.state.last_done_sec + EPSILON_SEC {
        emit_pct(ctx.sink, region.end_sec, ctx.trimmed_dur_sec);
        return Ok(());
    }
    st.slab_index += 1;
    let region_dur_sec = region.end_sec - region.start_sec;
    let resume_floor = st.state.last_done_sec;
    let t0 = std::time::Instant::now();
    let mut acc = ChunkAcc {
        state: &mut st.state,
        region_start_sec: region.start_sec,
        region_end_sec: region.end_sec,
        resume_floor,
        trimmed_dur_sec: ctx.trimmed_dur_sec,
        sink: ctx.sink,
        segs_emitted: 0,
        dropped: 0,
        save_err: None,
    };
    let mut sub_progress = |_pct: f64| {};
    let cancelled = || ctx.sink.is_cancelled();
    let (slab_detected, _rtf) = engine::run(
        &region.samples,
        region_dur_sec,
        ctx.config,
        &mut sub_progress,
        &cancelled,
        &mut |segs, end| acc.on_chunk(segs, end),
    )?;
    if let Some(e) = acc.save_err.take() {
        return Err(e);
    }
    let segs_emitted = acc.segs_emitted;
    let dropped = acc.dropped;

    if st.detected_language.is_empty() && !slab_detected.is_empty() {
        st.detected_language.clone_from(&slab_detected);
        logfile::info(&format!("detected language: {slab_detected}"));
    }
    let elapsed = t0.elapsed().as_secs_f64();
    st.total_audio += region_dur_sec;
    st.total_elapsed += elapsed;
    let slab_rtf = if elapsed > 0.0 {
        region_dur_sec / elapsed
    } else {
        0.0
    };
    logfile::info(&format!(
        "slab #{} {}-{} ({:.1}s in {:.1}s rtf={:.2}, {} segs{})",
        st.slab_index,
        format_hms(std::time::Duration::from_secs_f64(region.start_sec)),
        format_hms(std::time::Duration::from_secs_f64(region.end_sec)),
        region_dur_sec,
        elapsed,
        slab_rtf,
        segs_emitted,
        if dropped > 0 {
            format!(", dropped {dropped} dedup")
        } else {
            String::new()
        },
    ));
    Ok(())
}

fn run_streaming_phase(
    input: &Path,
    config: &Config,
    sink: &dyn Sink,
    key: &str,
    window: &TrimWindow,
) -> Result<(StreamState, f64)> {
    let mut st = StreamState {
        state: partial::load(key).unwrap_or_else(|| partial::Partial {
            key: key.to_string(),
            last_done_sec: 0.0,
            segments: Vec::new(),
        }),
        slab_index: 0,
        total_audio: 0.0,
        total_elapsed: 0.0,
        detected_language: String::new(),
    };
    let slab = slab_sec(config);
    if st.state.segments.is_empty() {
        logfile::info(&format!(
            "streaming start: slab={slab:.0}s engine={} model={}",
            config.engine.as_str(),
            config.model,
        ));
    } else {
        logfile::info(&format!(
            "resuming from {} ({} cached segs)",
            format_hms(std::time::Duration::from_secs_f64(st.state.last_done_sec)),
            st.state.segments.len(),
        ));
    }

    sink.phase(Phase::Transcribing);
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let cancel_for_stream = cancel_flag.clone();
    let pipeline_t0 = std::time::Instant::now();
    let ctx = StreamCtx {
        sink,
        config,
        trimmed_dur_sec: window.trimmed_dur_sec,
    };
    let scanned_end = stream_slabs(
        input,
        window.start_ms,
        window.end_ms_opt,
        slab,
        cancel_for_stream,
        |region| process_region(&ctx, &mut st, &region, &cancel_flag).map(|()| true),
    )?;
    if sink.is_cancelled() {
        return Err(crate::error::Error::Cancelled);
    }

    let observed_rtf = if st.total_elapsed > 0.0 {
        st.total_audio / st.total_elapsed
    } else {
        0.0
    };
    let device_label = config.device.as_str().to_owned();
    if observed_rtf > 0.0 {
        progress::save_rtf(&config.model, &device_label, observed_rtf);
    }
    logfile::info(&format!(
        "transcribed: {} slab(s) {:.1}s audio in {:.1}s rtf={:.2} (wall {:.1}s)",
        st.slab_index,
        st.total_audio,
        st.total_elapsed,
        observed_rtf,
        pipeline_t0.elapsed().as_secs_f64(),
    ));
    Ok((st, scanned_end))
}

fn run_diarize_phase(
    input: &Path,
    sink: &dyn Sink,
    config: &Config,
    speakers: u32,
    total_dur_ms: u64,
    scanned_end: f64,
) -> (Vec<DiarSegment>, Option<String>) {
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
    match run_diarize_streaming(input, diar_dur_sec, speakers, sink, config) {
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
        &config.model,
        &config.language,
        speakers,
        !config.diarize,
        trim.trim_start_ms,
        trim.trim_end_ms.unwrap_or(0),
    )?;
    let key = compute_key(&key_params);

    engine::preflight(config)?;

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
        ((scanned_end + window.start_ms as f64 / 1000.0) * 1000.0) as u64
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

struct BuildArgs<'a> {
    segments: &'a [Segment],
    diar_segs: &'a [DiarSegment],
    diar_name: Option<String>,
    config: &'a Config,
    key: &'a str,
    key_params: &'a cache::KeyParams,
    speakers: u32,
    language: String,
    device_label: String,
    duration_ms: u64,
}

fn build_and_cache_transcript(args: BuildArgs<'_>) -> Result<Transcript> {
    let transcript = transcript::build(
        args.segments,
        args.diar_segs,
        Meta {
            model: args.config.model.clone(),
            language: args.language.clone(),
            duration_ms: args.duration_ms,
            diarizer: args.diar_name,
            device: Some(args.device_label),
        },
    );
    let entry = CacheEntry {
        key: args.key.to_string(),
        source_path: args.key_params.source_path.clone(),
        source_name: args
            .key_params
            .source_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        model: args.config.model.clone(),
        language: args.language,
        speakers: args.speakers,
        no_diarize: !args.config.diarize,
        utterances: transcript.utterances.len(),
        duration_ms: args.duration_ms,
        created_at: Utc::now(),
        size_bytes: 0,
    };
    cache::store(entry, &transcript)?;
    let _ = partial::clear(args.key);
    logfile::info(&format!(
        "cache stored: key={} utterances={} speakers_detected={}",
        args.key,
        transcript.utterances.len(),
        transcript.speakers_detected,
    ));
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
    let segs = backend.diarize(&wav, speakers, audio_dur_sec, &cancelled, &mut on_progress)?;
    Ok((segs, backend_name))
}

fn apply_dedup(segments: &mut Vec<Segment>) {
    for seg in segments.iter_mut() {
        if seg.tokens.len() >= 2 {
            let before = seg.tokens.len();
            let collapsed = dedup::collapse_repeats(&seg.tokens);
            let after_adj = collapsed.len();
            let bridged = dedup::collapse_bridged_repeats(&collapsed);
            if bridged.len() != before {
                seg.tokens = bridged;
                rebuild_from_tokens(seg);
            }
            let _ = after_adj;
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

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn set_env(key: &str, value: &str) {
        unsafe {
            std::env::set_var(key, value);
        }
    }

    fn unset_env(key: &str) {
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

        let config = Config::default();
        unset_env("WT_SLAB_SEC");
        assert!((slab_sec(&config) - DEFAULT_SLAB_SEC).abs() < f64::EPSILON);

        for invalid in ["0", "-5", "not-a-number"] {
            set_env("WT_SLAB_SEC", invalid);
            assert!(
                (slab_sec(&config) - DEFAULT_SLAB_SEC).abs() < f64::EPSILON,
                "input {invalid:?} should fall back to default"
            );
        }

        set_env("WT_SLAB_SEC", "30");
        assert!((slab_sec(&config) - 30.0).abs() < f64::EPSILON);

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

        assert_eq!(segs[0].tokens.len(), 1);
        assert_eq!(segs[0].text, "the");
        assert_eq!(segs[0].start_ms, 100);
        assert_eq!(segs[0].end_ms, 200);
    }

    #[test]
    fn apply_dedup_collapses_in_plain_text_when_no_tokens() {
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
