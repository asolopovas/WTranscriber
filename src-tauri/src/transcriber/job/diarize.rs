use std::path::Path;

use crate::{
    audio,
    config::Config,
    diarizer::{self, Segment as DiarSegment},
    error::Result,
    logfile,
    progress::{Phase, Sink},
};

pub(super) fn run_diarize_phase(
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
