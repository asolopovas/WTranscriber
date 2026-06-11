use std::{
    path::Path,
    sync::{Arc, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use tokio_util::sync::CancellationToken;

use std::sync::atomic::{AtomicBool, Ordering};

use crate::{
    audio_toolkit::stream::stream_slabs,
    config::Config,
    engine,
    error::Result,
    logfile,
    progress::{self, Phase, Sink},
    transcriber::{format::format_hms, partial, transcript::Segment},
};

use super::{
    postprocess::{apply_dedup, shift_segments},
    slab::{first_slab_sec, slab_sec},
    trim::TrimWindow,
};

const EPSILON_SEC: f64 = 1e-3;

struct StreamCtx<'a> {
    sink: &'a dyn Sink,
    config: &'a Config,
    trimmed_dur_sec: f64,
}

pub(super) struct StreamState {
    pub(super) state: partial::Partial,
    slab_index: usize,
    total_audio: f64,
    total_elapsed: f64,
    pub(super) detected_language: String,
}

struct SlabHeartbeat {
    stop: mpsc::Sender<()>,
    handle: Option<JoinHandle<()>>,
}

impl SlabHeartbeat {
    fn start(index: usize, start: String, end: String, cancel: Option<CancellationToken>) -> Self {
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let t0 = Instant::now();
            loop {
                match rx.recv_timeout(Duration::from_secs(15)) {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
                            break;
                        }
                        logfile::debug(&format!(
                            "slab #{index} processing {start}-{end} ({:.0}s elapsed)",
                            t0.elapsed().as_secs_f64()
                        ));
                    }
                }
            }
        });
        Self {
            stop: tx,
            handle: Some(handle),
        }
    }

    fn stop(mut self) {
        let _ = self.stop.send(());
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
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
        let emitted = segs.len();
        self.state.segments.extend(segs);
        self.segs_emitted += emitted;
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
    let start_label = format_hms(std::time::Duration::from_secs_f64(region.start_sec));
    let end_label = format_hms(std::time::Duration::from_secs_f64(region.end_sec));
    if skip_no_speech(ctx, st, region, &start_label, &end_label)? {
        return Ok(());
    }
    logfile::info(&format!(
        "slab #{} start {}-{} ({region_dur_sec:.1}s audio)",
        st.slab_index, start_label, end_label
    ));
    let heartbeat = ctx.config.debug_logging.then(|| {
        SlabHeartbeat::start(
            st.slab_index,
            start_label.clone(),
            end_label.clone(),
            ctx.sink.cancellation_token(),
        )
    });
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
    let mut last_progress_log = 0_i32;
    let mut sub_progress = |pct: f64| {
        let pct = pct.clamp(0.0, 100.0);
        let abs_end = region.start_sec + region_dur_sec * pct / 100.0;
        emit_pct(ctx.sink, abs_end, ctx.trimmed_dur_sec);
        let step = ((pct / 25.0).floor() as i32) * 25;
        if ctx.config.debug_logging && step > last_progress_log && step < 100 {
            last_progress_log = step;
            logfile::debug(&format!("slab #{} progress {step}%", st.slab_index));
        }
    };
    let cancelled = || ctx.sink.is_cancelled();
    let run_result = engine::run(
        &region.samples,
        region_dur_sec,
        ctx.config,
        &mut sub_progress,
        &cancelled,
        &mut |segs, end| acc.on_chunk(segs, end),
    );
    if let Some(heartbeat) = heartbeat {
        heartbeat.stop();
    }
    let (slab_detected, _rtf) = run_result?;
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
        start_label,
        end_label,
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

pub(super) fn run_streaming_phase(
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
    let first_slab = first_slab_sec(config, window.trimmed_dur_sec);
    if st.state.segments.is_empty() {
        logfile::info(&format!(
            "streaming start: slab={slab:.0}s first={first_slab:.0}s engine={} model={}",
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
        first_slab,
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

fn skip_no_speech(
    ctx: &StreamCtx<'_>,
    st: &mut StreamState,
    region: &crate::audio_toolkit::vad::Region,
    start_label: &str,
    end_label: &str,
) -> Result<bool> {
    if slab_has_speech(&region.samples) {
        return Ok(false);
    }
    logfile::info(&format!(
        "slab #{} {}-{} skipped (no speech detected)",
        st.slab_index, start_label, end_label
    ));
    st.state.last_done_sec = region.end_sec;
    partial::save(&st.state)?;
    emit_pct(ctx.sink, region.end_sec, ctx.trimmed_dur_sec);
    Ok(true)
}

const VAD_SPEECH_FRAMES: usize = 3;

fn slab_has_speech(samples: &[f32]) -> bool {
    use crate::audio_toolkit::{
        constants::FRAME_SAMPLES,
        vad::{self, VoiceActivityDetector as _},
    };
    if std::env::var("WT_NO_VAD_GATE")
        .ok()
        .is_some_and(|v| v == "1")
    {
        return true;
    }
    let Ok(model) = vad::model::model_path() else {
        return true;
    };
    if !model.exists() {
        return true;
    }
    let Ok(mut detector) = vad::SileroVad::new(&model, 0.5) else {
        return true;
    };
    let mut speech_frames = 0usize;
    for frame in samples.chunks_exact(FRAME_SAMPLES) {
        match detector.push_frame(frame) {
            Ok(vad::VadFrame::Speech(_)) => {
                speech_frames += 1;
                if speech_frames >= VAD_SPEECH_FRAMES {
                    return true;
                }
            }
            Ok(vad::VadFrame::Noise) => {}
            Err(_) => return true,
        }
    }
    speech_frames > 0
}

fn emit_pct(sink: &dyn Sink, done_sec: f64, total_sec: f64) {
    if total_sec <= 0.0 {
        return;
    }
    let pct = (done_sec / total_sec * 100.0).clamp(0.0, 99.9);
    sink.report_pct(Phase::Transcribing, pct);
}
