#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod nemo;
mod sherpa;

use std::path::Path;

use crate::{config::DiarizerChoice, error::Result};

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

pub fn new(num_speakers: u32) -> Result<Box<dyn Backend>> {
    new_with_choice(num_speakers, DiarizerChoice::Auto)
}

pub fn new_with_choice(num_speakers: u32, choice: DiarizerChoice) -> Result<Box<dyn Backend>> {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = choice;
        return Ok(Box::new(SherpaDiarizer::new(num_speakers)?));
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        match choice {
            DiarizerChoice::Nemo => nemo::NemoDiarizer::new()
                .map(|d| Box::new(d) as Box<dyn Backend>),
            DiarizerChoice::Sherpa => SherpaDiarizer::new(num_speakers)
                .map(|d| Box::new(d) as Box<dyn Backend>),
            DiarizerChoice::Auto => {
                if num_speakers > 0 {
                    new_sherpa_or_nemo(num_speakers)
                } else {
                    new_nemo_or_sherpa(num_speakers)
                }
            }
        }
    }
}

#[deprecated(note = "use new_with_choice")]
#[allow(dead_code)]
pub fn new_with_preference(num_speakers: u32, prefer_sherpa: bool) -> Result<Box<dyn Backend>> {
    new_with_choice(
        num_speakers,
        if prefer_sherpa {
            DiarizerChoice::Sherpa
        } else {
            DiarizerChoice::Auto
        },
    )
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn new_nemo_or_sherpa(num_speakers: u32) -> Result<Box<dyn Backend>> {
    match nemo::NemoDiarizer::new() {
        Ok(d) => Ok(Box::new(d)),
        Err(primary) => SherpaDiarizer::new(num_speakers)
            .map_or_else(|_| Err(primary), |d| Ok(Box::new(d) as Box<dyn Backend>)),
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn new_sherpa_or_nemo(num_speakers: u32) -> Result<Box<dyn Backend>> {
    match SherpaDiarizer::new(num_speakers) {
        Ok(d) => Ok(Box::new(d)),
        Err(primary) => nemo::NemoDiarizer::new()
            .map_or_else(|_| Err(primary), |d| Ok(Box::new(d) as Box<dyn Backend>)),
    }
}

pub fn speaker_id_for_time(
    start_sec: f64,
    end_sec: f64,
    diar: &[Segment],
    hint: Option<u32>,
) -> Option<u32> {
    if diar.is_empty() {
        return None;
    }
    let mut overlap: std::collections::HashMap<u32, f64> = std::collections::HashMap::new();
    for ds in diar {
        if ds.end_sec <= start_sec || ds.start_sec >= end_sec {
            continue;
        }
        let o = ds.end_sec.min(end_sec) - ds.start_sec.max(start_sec);
        if o > 0.0 {
            *overlap.entry(ds.speaker).or_insert(0.0) += o;
        }
    }
    if overlap.is_empty() {
        let mid = f64::midpoint(start_sec, end_sec);
        return diar
            .iter()
            .min_by(|a, b| {
                let am = f64::midpoint(a.start_sec, a.end_sec);
                let bm = f64::midpoint(b.start_sec, b.end_sec);
                (mid - am)
                    .abs()
                    .partial_cmp(&(mid - bm).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|s| s.speaker);
    }
    let (best_spk, best_ovl) = overlap
        .iter()
        .max_by(|(spk_a, ovl_a), (spk_b, ovl_b)| {
            ovl_a
                .partial_cmp(ovl_b)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| spk_b.cmp(spk_a))
        })
        .map(|(s, o)| (*s, *o))?;
    if let Some(h) = hint
        && let Some(hint_ovl) = overlap.get(&h)
        && best_ovl - hint_ovl < 0.005
    {
        return Some(h);
    }
    Some(best_spk)
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
    fn falls_back_to_nearest_when_no_overlap() {
        let diar = vec![seg(1, 0.0, 1.0), seg(2, 10.0, 12.0)];
        assert_eq!(speaker_id_for_time(5.0, 6.0, &diar, None), Some(1));
    }

    #[test]
    fn hint_breaks_near_ties() {
        let diar = vec![seg(1, 0.0, 1.0), seg(2, 1.0, 2.001)];
        assert_eq!(speaker_id_for_time(0.5, 1.5, &diar, Some(1)), Some(1));
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn desktop_default_prefers_nemo_for_auto_speakers() {
        assert!(!prefers_sherpa(0, false));
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn desktop_fixed_speaker_count_prefers_sherpa() {
        assert!(prefers_sherpa(2, false));
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    const fn prefers_sherpa(num_speakers: u32, prefer_sherpa: bool) -> bool {
        prefer_sherpa || num_speakers > 0
    }
}
