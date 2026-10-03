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
    trim: &audio::AudioMeta,
) -> (Vec<DiarSegment>, Option<String>) {
    sink.phase(Phase::Diarizing);
    let diar_t0 = std::time::Instant::now();
    logfile::info(&format!(
        "diarize start: backend={} speakers={}",
        config.diarizer.as_str(),
        speakers,
    ));
    match run_diarize_streaming(input, trim, speakers, sink, config) {
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
    trim: &audio::AudioMeta,
    speakers: u32,
    sink: &dyn Sink,
    config: &Config,
) -> Result<(Vec<DiarSegment>, String)> {
    let backend = diarizer::new_with_choice(speakers, config.diarizer, config.device)?;
    let backend_name = backend.name();
    sink.set_diarize_backend(&backend_name);
    let segs = diarizer::trimmed::run(backend.as_ref(), input, trim, speakers, sink)?;
    Ok((segs, backend_name))
}
