use std::path::{Path, PathBuf};
use std::sync::Mutex;

use parakeet_rs::sortformer::{DiarizationConfig, Sortformer};

use crate::{
    audio::decode,
    diarizer::{Backend, Progress, Segment},
    error::{Error, Result},
    paths,
};

const MODEL_REL: &str = "sortformer-v2-onnx/model.onnx";
const SAMPLE_RATE: i32 = 16_000;

pub struct SortformerDiarizer {
    model_path: PathBuf,
    inner: Mutex<Sortformer>,
}

impl SortformerDiarizer {
    pub fn new() -> Result<Self> {
        let model_path = paths::models_dir()?.join(MODEL_REL);
        if !model_path.exists() {
            return Err(Error::Transcribe(format!(
                "sortformer-onnx model missing at {}",
                model_path.display()
            )));
        }
        let exec_cfg = sortformer_exec_config();
        let sf = Sortformer::with_config(&model_path, exec_cfg, DiarizationConfig::callhome())
            .map_err(|e| Error::Transcribe(format!("sortformer load: {e}")))?;
        Ok(Self {
            model_path,
            inner: Mutex::new(sf),
        })
    }
}

#[cfg(feature = "cuda")]
#[allow(clippy::unnecessary_wraps)]
fn sortformer_exec_config() -> Option<parakeet_rs::ExecutionConfig> {
    use parakeet_rs::{ExecutionConfig, ExecutionProvider};
    Some(ExecutionConfig::new().with_execution_provider(ExecutionProvider::Cuda))
}

#[cfg(not(feature = "cuda"))]
const fn sortformer_exec_config() -> Option<parakeet_rs::ExecutionConfig> {
    None
}

impl Backend for SortformerDiarizer {
    fn name(&self) -> String {
        format!(
            "sortformer-onnx-v2.1+{}",
            self.model_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("model")
        )
    }

    fn diarize(
        &self,
        wav: &Path,
        _num_speakers: u32,
        _audio_dur_sec: f64,
        cancelled: &dyn Fn() -> bool,
        on_progress: Progress<'_>,
    ) -> Result<Vec<Segment>> {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        on_progress(0.0);
        let samples = decode::decode_to_pcm_f32(wav, SAMPLE_RATE)?;
        if cancelled() {
            return Err(Error::Cancelled);
        }
        on_progress(0.1);

        let mut guard = self
            .inner
            .lock()
            .map_err(|e| Error::Transcribe(format!("sortformer lock poisoned: {e}")))?;
        #[allow(clippy::cast_sign_loss)]
        let segs = guard
            .diarize(samples, SAMPLE_RATE as u32, 1)
            .map_err(|e| Error::Transcribe(format!("sortformer diarize: {e}")))?;
        drop(guard);

        if cancelled() {
            return Err(Error::Cancelled);
        }
        on_progress(0.95);

        #[allow(clippy::cast_precision_loss)]
        let out = segs
            .into_iter()
            .map(|s| Segment {
                speaker: u32::try_from(s.speaker_id).unwrap_or(u32::MAX),
                start_sec: s.start as f64 / f64::from(SAMPLE_RATE),
                end_sec: s.end as f64 / f64::from(SAMPLE_RATE),
            })
            .collect();
        on_progress(1.0);
        Ok(out)
    }
}
