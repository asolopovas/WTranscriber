#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::path::{Path, PathBuf};

use sherpa_onnx::{
    FastClusteringConfig, OfflineSpeakerDiarization, OfflineSpeakerDiarizationConfig,
    OfflineSpeakerSegmentationModelConfig, OfflineSpeakerSegmentationPyannoteModelConfig,
    SpeakerEmbeddingExtractorConfig,
};

use crate::{
    audio::decode,
    diarizer::{Backend, Progress, Segment},
    error::{Error, Result},
    paths,
};

const SEG_REL: &str = "sherpa-onnx-pyannote-segmentation-3-0/model.onnx";

#[derive(Debug, Clone)]
pub struct SherpaDiarizer {
    seg_model: PathBuf,
    emb_model: PathBuf,
    num_speakers: u32,
}

fn resolve_models(emb_rel: &str) -> Result<(PathBuf, PathBuf)> {
    let root = paths::models_dir()?;
    let seg = root.join(SEG_REL.replace('/', std::path::MAIN_SEPARATOR_STR));
    let emb = root.join(emb_rel);
    if !seg.exists() {
        return Err(Error::Transcribe(format!(
            "diarizer segmentation model missing at {}",
            seg.display()
        )));
    }
    if !emb.exists() {
        return Err(Error::Transcribe(format!(
            "diarizer embedding model missing at {}",
            emb.display()
        )));
    }
    Ok((seg, emb))
}

fn diarizer_threads() -> i32 {
    let n = std::thread::available_parallelism().map_or(4, std::num::NonZero::get) / 2;
    i32::try_from(n).unwrap_or(4).clamp(2, 8)
}

impl SherpaDiarizer {
    pub fn new(num_speakers: u32, emb_rel: &str) -> Result<Self> {
        let (seg_model, emb_model) = resolve_models(emb_rel)?;
        Ok(Self {
            seg_model,
            emb_model,
            num_speakers,
        })
    }
}

impl Backend for SherpaDiarizer {
    fn name(&self) -> String {
        format!(
            "sherpa-onnx-pyannote+{}",
            self.emb_model
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("embedding")
        )
    }

    fn diarize(
        &self,
        wav: &Path,
        num_speakers: u32,
        _audio_dur_sec: f64,
        cancelled: &dyn Fn() -> bool,
        on_progress: Progress<'_>,
    ) -> Result<Vec<Segment>> {
        let threads = diarizer_threads();
        let n_clusters = i32::try_from(if num_speakers > 0 {
            num_speakers
        } else {
            self.num_speakers
        })
        .unwrap_or(0);
        let config = OfflineSpeakerDiarizationConfig {
            segmentation: OfflineSpeakerSegmentationModelConfig {
                pyannote: OfflineSpeakerSegmentationPyannoteModelConfig {
                    model: Some(self.seg_model.to_string_lossy().into_owned()),
                },
                num_threads: threads,
                debug: false,
                provider: Some("cpu".into()),
            },
            embedding: SpeakerEmbeddingExtractorConfig {
                model: Some(self.emb_model.to_string_lossy().into_owned()),
                num_threads: threads,
                debug: false,
                provider: Some("cpu".into()),
            },
            clustering: FastClusteringConfig {
                num_clusters: n_clusters,
                threshold: 0.5,
            },
            min_duration_on: 0.2,
            min_duration_off: 0.2,
        };

        let sd = OfflineSpeakerDiarization::create(&config)
            .ok_or_else(|| Error::Transcribe("diarizer init failed".into()))?;

        let samples = decode::decode_to_pcm_f32(wav, sd.sample_rate())?;

        if cancelled() {
            return Err(Error::Transcribe("cancelled".into()));
        }

        let result = sd
            .process(&samples)
            .ok_or_else(|| Error::Transcribe("diarize failed".into()))?;
        on_progress(100.0);

        let mut segs: Vec<Segment> = result
            .sort_by_start_time()
            .into_iter()
            .map(|s| Segment {
                speaker: u32::try_from(s.speaker).unwrap_or(0),
                start_sec: f64::from(s.start),
                end_sec: f64::from(s.end),
            })
            .collect();
        segs.sort_by(|a, b| {
            a.start_sec
                .partial_cmp(&b.start_sec)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(segs)
    }
}
