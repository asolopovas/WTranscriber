mod sherpa;
#[cfg(not(target_os = "ios"))]
mod sortformer_onnx;
pub mod trimmed;

use std::path::Path;

use crate::{
    config::{Device, DiarizerChoice},
    error::Result,
};

pub use sherpa::SherpaDiarizer;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    pub speaker: u32,
    pub start_sec: f64,
    pub end_sec: f64,
}

pub type Progress<'a> = &'a mut dyn FnMut(f64);

pub trait Backend {
    fn name(&self) -> String;
    fn diarize(
        &self,
        wav: &Path,
        num_speakers: u32,
        audio_dur_sec: f64,
        cancelled: &dyn Fn() -> bool,
        on_progress: Progress<'_>,
    ) -> Result<Vec<Segment>>;
}

pub fn new_with_choice(
    num_speakers: u32,
    choice: DiarizerChoice,
    device: Device,
) -> Result<Box<dyn Backend>> {
    let titanet_fallback = || {
        SherpaDiarizer::new(
            num_speakers,
            DiarizerChoice::Titanet.embedding_rel(),
            device,
        )
        .map(|d| Box::new(d) as Box<dyn Backend>)
    };
    match choice {
        DiarizerChoice::WhisperxCommunity1 => Err(crate::error::Error::Config(
            "WhisperX + Community-1 must run through the quality pipeline".into(),
        )),
        DiarizerChoice::SortformerOnnx => {
            #[cfg(target_os = "ios")]
            {
                let _ = num_speakers;
                titanet_fallback()
            }
            #[cfg(not(target_os = "ios"))]
            {
                if num_speakers > 4 {
                    crate::logfile::info(&format!(
                        "sortformer-onnx supports max 4 speakers; {num_speakers} requested, using titanet"
                    ));
                    return titanet_fallback();
                }
                match sortformer_onnx::SortformerDiarizer::new(device) {
                    Ok(d) => Ok(Box::new(d) as Box<dyn Backend>),
                    Err(e) => {
                        crate::logfile::warn(&format!(
                            "diarizer sortformer-onnx failed at init ({e}); falling back to titanet"
                        ));
                        titanet_fallback()
                    }
                }
            }
        }
        DiarizerChoice::Titanet => {
            SherpaDiarizer::new(num_speakers, choice.embedding_rel(), device)
                .map(|d| Box::new(d) as Box<dyn Backend>)
        }
    }
}

pub fn speaker_id_for_time(
    start_sec: f64,
    end_sec: f64,
    diar: &[Segment],
    hint: Option<u32>,
) -> Option<u32> {
    if !start_sec.is_finite() || !end_sec.is_finite() || end_sec < start_sec {
        return None;
    }
    let mut spans: std::collections::BTreeMap<u32, Vec<(f64, f64)>> =
        std::collections::BTreeMap::new();
    for segment in diar {
        if !segment.start_sec.is_finite()
            || !segment.end_sec.is_finite()
            || segment.end_sec <= segment.start_sec
        {
            continue;
        }
        let start = segment.start_sec.max(start_sec);
        let end = segment.end_sec.min(end_sec);
        if end > start {
            spans.entry(segment.speaker).or_default().push((start, end));
        }
    }
    let mut overlap = std::collections::BTreeMap::new();
    for (speaker, mut ranges) in spans {
        ranges.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut covered_end = f64::NEG_INFINITY;
        let mut duration = 0.0;
        for (start, end) in ranges {
            duration += (end - start.max(covered_end)).max(0.0);
            covered_end = covered_end.max(end);
        }
        overlap.insert(speaker, duration);
    }
    let (best, duration) =
        overlap
            .iter()
            .max_by(|(speaker_a, duration_a), (speaker_b, duration_b)| {
                duration_a
                    .total_cmp(duration_b)
                    .then_with(|| speaker_b.cmp(speaker_a))
            })?;
    if let Some(hint) = hint
        && overlap
            .get(&hint)
            .is_some_and(|hint_duration| (duration - hint_duration).abs() < 1e-9)
    {
        return Some(hint);
    }
    Some(*best)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(speaker: u32, start: f64, end: f64) -> Segment {
        Segment {
            speaker,
            start_sec: start,
            end_sec: end,
        }
    }

    #[test]
    fn picks_speaker_with_max_overlap() {
        let diar = vec![seg(1, 0.0, 5.0), seg(2, 5.0, 10.0)];
        assert_eq!(speaker_id_for_time(2.0, 4.0, &diar, None), Some(1));
        assert_eq!(speaker_id_for_time(6.0, 9.0, &diar, None), Some(2));
    }

    #[test]
    fn leaves_speech_unassigned_when_no_turn_overlaps() {
        let diar = vec![seg(1, 0.0, 1.0), seg(2, 10.0, 12.0)];
        assert_eq!(speaker_id_for_time(5.0, 6.0, &diar, None), None);
    }

    #[test]
    fn hint_breaks_near_ties() {
        let diar = vec![seg(1, 0.0, 1.0), seg(2, 1.0, 2.001)];
        assert_eq!(speaker_id_for_time(0.5, 1.5, &diar, Some(1)), Some(1));
    }

    #[test]
    fn duplicate_turns_do_not_outvote_the_actual_speaker() {
        let diar = vec![seg(1, 0.0, 0.6), seg(1, 0.0, 0.6), seg(2, 0.2, 1.0)];
        assert_eq!(speaker_id_for_time(0.0, 1.0, &diar, Some(1)), Some(2));
    }

    #[test]
    fn previous_speaker_does_not_override_a_short_response() {
        let diar = vec![seg(1, 0.0, 0.499), seg(2, 0.499, 1.0)];
        assert_eq!(speaker_id_for_time(0.0, 1.0, &diar, Some(1)), Some(2));
        assert_eq!(speaker_id_for_time(0.0, 0.0, &diar, None), None);
    }

    #[cfg(not(target_os = "ios"))]
    #[test]
    fn desktop_default_prefers_nemo_for_auto_speakers() {
        assert!(!prefers_sherpa(0, false));
    }

    #[cfg(not(target_os = "ios"))]
    #[test]
    fn desktop_fixed_speaker_count_prefers_sherpa() {
        assert!(prefers_sherpa(2, false));
    }

    #[cfg(not(target_os = "ios"))]
    const fn prefers_sherpa(num_speakers: u32, prefer_sherpa: bool) -> bool {
        prefer_sherpa || num_speakers > 0
    }
}
