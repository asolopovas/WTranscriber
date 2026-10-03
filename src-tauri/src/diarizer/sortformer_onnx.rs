use std::path::{Path, PathBuf};
use std::sync::Mutex;

use parakeet_rs::sortformer::{DiarizationConfig, Sortformer};

use crate::{
    audio::decode,
    config::Device,
    diarizer::{Backend, Progress, Segment},
    error::{Error, Result},
    paths,
};

const MODEL_REL: &str = "sortformer-v2-onnx/model.onnx";
const SAMPLE_RATE: i32 = 16_000;

pub struct SortformerDiarizer {
    model_path: PathBuf,
    inner: Mutex<Sortformer>,
    cpu_fallback: Mutex<Option<Sortformer>>,
}

impl SortformerDiarizer {
    pub fn new(device: Device) -> Result<Self> {
        let model_path = paths::models_dir()?.join(MODEL_REL);
        if !model_path.exists() {
            return Err(Error::Transcribe(format!(
                "sortformer-onnx model missing at {}",
                model_path.display()
            )));
        }
        let exec_cfg = sortformer_exec_config(device);
        let accelerated = exec_cfg.is_some();
        let sf = Sortformer::with_config(&model_path, exec_cfg, DiarizationConfig::callhome())
            .or_else(|error| {
                if !accelerated {
                    return Err(error);
                }
                crate::logfile::warn(&format!(
                    "sortformer GPU initialisation failed ({error}); retrying on CPU"
                ));
                Sortformer::with_config(&model_path, None, DiarizationConfig::callhome())
            })
            .map_err(|e| Error::Transcribe(format!("sortformer load: {e}")))?;
        Ok(Self {
            model_path,
            inner: Mutex::new(sf),
            cpu_fallback: Mutex::new(None),
        })
    }
}

#[cfg(all(windows, feature = "directml"))]
#[allow(clippy::unnecessary_wraps)]
fn sortformer_exec_config(device: Device) -> Option<parakeet_rs::ExecutionConfig> {
    use parakeet_rs::{ExecutionConfig, ExecutionProvider};
    if matches!(device, Device::Cpu) {
        return None;
    }
    Some(ExecutionConfig::new().with_execution_provider(ExecutionProvider::DirectML))
}

#[cfg(all(feature = "cuda", not(all(windows, feature = "directml"))))]
#[allow(clippy::unnecessary_wraps)]
fn sortformer_exec_config(device: Device) -> Option<parakeet_rs::ExecutionConfig> {
    use parakeet_rs::{ExecutionConfig, ExecutionProvider};
    if matches!(device, Device::Cpu) {
        return None;
    }
    Some(ExecutionConfig::new().with_execution_provider(ExecutionProvider::Cuda))
}

#[cfg(not(any(feature = "cuda", all(windows, feature = "directml"))))]
const fn sortformer_exec_config(_device: Device) -> Option<parakeet_rs::ExecutionConfig> {
    None
}

fn is_cuda_kernel_image_error(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("nokernelimagefordevice")
        || message.contains("no kernel image is available for execution on the device")
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
        let segs = match guard.diarize(samples.clone(), SAMPLE_RATE as u32, 1) {
            Ok(segs) => segs,
            Err(e) if is_cuda_kernel_image_error(&e.to_string()) => {
                drop(guard);
                crate::logfile::warn(
                    "sortformer CUDA unavailable for this GPU; retrying diarization on CPU",
                );
                let mut cpu = self
                    .cpu_fallback
                    .lock()
                    .map_err(|e| Error::Transcribe(format!("sortformer cpu lock poisoned: {e}")))?;
                if cpu.is_none() {
                    *cpu = Some(
                        Sortformer::with_config(
                            &self.model_path,
                            None,
                            DiarizationConfig::callhome(),
                        )
                        .map_err(|e| Error::Transcribe(format!("sortformer cpu load: {e}")))?,
                    );
                }
                cpu.as_mut()
                    .ok_or_else(|| Error::Transcribe("sortformer cpu not initialised".into()))?
                    .diarize(samples, SAMPLE_RATE as u32, 1)
                    .map_err(|e| Error::Transcribe(format!("sortformer cpu diarize: {e}")))?
            }
            Err(e) => return Err(Error::Transcribe(format!("sortformer diarize: {e}"))),
        };

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_config_respects_requested_device() {
        assert!(sortformer_exec_config(Device::Cpu).is_none());
        assert_eq!(
            sortformer_exec_config(Device::Cuda).is_some(),
            cfg!(any(feature = "cuda", all(windows, feature = "directml")))
        );
    }

    #[test]
    #[ignore = "requires WT_TEST_SORTFORMER_MODEL pointing to the installed four-speaker model"]
    fn installed_four_speaker_model_loads_and_processes_audio() {
        let path = std::env::var_os("WT_TEST_SORTFORMER_MODEL").unwrap();
        let mut model =
            Sortformer::with_config(PathBuf::from(path), None, DiarizationConfig::callhome())
                .unwrap();
        let segments = model.diarize(vec![0.0; 16_000], 16_000, 1).unwrap();
        assert!(segments.iter().all(|segment| segment.speaker_id < 4));
    }
}
