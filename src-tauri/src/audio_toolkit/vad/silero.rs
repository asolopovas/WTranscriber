use std::path::Path;

use ort::{
    session::{Session, SessionOutputs, builder::GraphOptimizationLevel},
    value::TensorRef,
};

use super::{VadFrame, VoiceActivityDetector};
use crate::{
    audio_toolkit::constants::{FRAME_SAMPLES, WHISPER_SAMPLE_RATE},
    error::{Error, Result},
};

const STATE_SHAPE: [i64; 3] = [2, 1, 64];
const STATE_SAMPLES: usize = 128;

pub struct SileroVad {
    session: Session,
    hidden: [f32; STATE_SAMPLES],
    cell: [f32; STATE_SAMPLES],
    threshold: f32,
}

impl SileroVad {
    pub fn new<P: AsRef<Path>>(model_path: P, threshold: f32) -> Result<Self> {
        if !(0.0..=1.0).contains(&threshold) {
            return Err(Error::Config(
                "vad threshold must be between 0.0 and 1.0".into(),
            ));
        }
        let session = (|| -> ort::Result<Session> {
            Session::builder()?
                .with_optimization_level(GraphOptimizationLevel::Level3)?
                .with_intra_threads(1)?
                .with_inter_threads(1)?
                .commit_from_file(model_path.as_ref())
        })()
        .map_err(|e| Error::Config(format!("silero vad: {e}")))?;
        Ok(Self {
            session,
            hidden: [0.0; STATE_SAMPLES],
            cell: [0.0; STATE_SAMPLES],
            threshold,
        })
    }

    fn probability(&mut self, frame: &[f32]) -> Result<f32> {
        let sample_rate = [i64::from(WHISPER_SAMPLE_RATE)];
        let input = TensorRef::from_array_view(([1, FRAME_SAMPLES], frame))
            .map_err(|error| inference_error(&error))?;
        let rate = TensorRef::from_array_view(([1], &sample_rate[..]))
            .map_err(|error| inference_error(&error))?;
        let hidden = TensorRef::from_array_view((STATE_SHAPE, &self.hidden[..]))
            .map_err(|error| inference_error(&error))?;
        let cell = TensorRef::from_array_view((STATE_SHAPE, &self.cell[..]))
            .map_err(|error| inference_error(&error))?;
        let outputs = self
            .session
            .run(ort::inputs![
                "input" => input,
                "sr" => rate,
                "h" => hidden,
                "c" => cell,
            ])
            .map_err(|error| inference_error(&error))?;
        let next_hidden = output_array::<STATE_SAMPLES>(&outputs, "hn", &STATE_SHAPE)?;
        let next_cell = output_array::<STATE_SAMPLES>(&outputs, "cn", &STATE_SHAPE)?;
        let [probability] = output_array::<1>(&outputs, "output", &[1, 1])?;
        if !(0.0..=1.0).contains(&probability) {
            return Err(Error::Transcribe(
                "silero output probability is invalid".into(),
            ));
        }
        self.hidden = next_hidden;
        self.cell = next_cell;
        Ok(probability)
    }
}

fn inference_error(error: &ort::Error) -> Error {
    Error::Transcribe(format!("silero compute: {error}"))
}

fn output_array<const N: usize>(
    outputs: &SessionOutputs<'_>,
    name: &str,
    expected_shape: &[i64],
) -> Result<[f32; N]> {
    let output = outputs
        .get(name)
        .ok_or_else(|| Error::Transcribe(format!("silero output missing: {name}")))?;
    let (shape, data) = output
        .try_extract_tensor::<f32>()
        .map_err(|error| inference_error(&error))?;
    if &shape[..] != expected_shape {
        return Err(Error::Transcribe(format!(
            "silero output {name} has unexpected shape: {shape:?}"
        )));
    }
    data.try_into().map_err(|_| {
        Error::Transcribe(format!(
            "silero output {name} has unexpected length: {}",
            data.len()
        ))
    })
}

impl VoiceActivityDetector for SileroVad {
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>> {
        if frame.len() != FRAME_SAMPLES {
            return Err(Error::Transcribe(format!(
                "vad frame size mismatch: expected {FRAME_SAMPLES}, got {}",
                frame.len()
            )));
        }
        if self.probability(frame)? > self.threshold {
            Ok(VadFrame::Speech(frame))
        } else {
            Ok(VadFrame::Noise)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_threshold_is_rejected_before_loading_model() {
        for threshold in [-0.1, 1.1, f32::NAN] {
            assert!(matches!(
                SileroVad::new("missing.onnx", threshold),
                Err(Error::Config(_))
            ));
        }
    }

    #[test]
    #[ignore = "requires WT_TEST_VAD_MODEL pointing to the Silero v4 model"]
    fn recurrent_inference_matches_owned_tensor_reference() {
        use ort::value::Tensor;

        let model = std::env::var("WT_TEST_VAD_MODEL").expect("set WT_TEST_VAD_MODEL");
        let mut vad = SileroVad::new(&model, 0.5).unwrap();
        let mut reference = SileroVad::new(&model, 0.5).unwrap();
        let mut hidden = vec![0.0_f32; STATE_SAMPLES];
        let mut cell = vec![0.0_f32; STATE_SAMPLES];
        for index in 0..24 {
            let frame = (0..FRAME_SAMPLES)
                .map(|sample| {
                    if index < 4 {
                        0.0
                    } else {
                        #[allow(clippy::cast_precision_loss)]
                        let phase = (sample + index * FRAME_SAMPLES) as f32 * 0.1;
                        phase.sin() * 0.25
                    }
                })
                .collect::<Vec<_>>();
            let outputs = reference.session.run(ort::inputs![
                "input" => Tensor::from_array(([1, FRAME_SAMPLES], frame.clone())).unwrap(),
                "sr" => Tensor::from_array(([1], vec![i64::from(WHISPER_SAMPLE_RATE)])).unwrap(),
                "h" => Tensor::from_array((STATE_SHAPE, hidden.clone())).unwrap(),
                "c" => Tensor::from_array((STATE_SHAPE, cell.clone())).unwrap(),
            ]).unwrap();
            let expected = outputs["output"].try_extract_tensor::<f32>().unwrap().1[0];
            hidden = outputs["hn"]
                .try_extract_tensor::<f32>()
                .unwrap()
                .1
                .to_vec();
            cell = outputs["cn"]
                .try_extract_tensor::<f32>()
                .unwrap()
                .1
                .to_vec();
            let actual = vad.probability(&frame).unwrap();
            assert!(
                (actual - expected).abs() < 1e-6,
                "frame {index}: {actual} != {expected}"
            );
            assert_eq!(vad.hidden.as_slice(), hidden.as_slice());
            assert_eq!(vad.cell.as_slice(), cell.as_slice());
        }
        assert!(matches!(
            vad.push_frame(&[0.0; 1]),
            Err(Error::Transcribe(_))
        ));
    }

    #[test]
    fn missing_model_returns_configuration_error() {
        assert!(matches!(
            SileroVad::new("missing.onnx", 0.5),
            Err(Error::Config(_))
        ));
    }
}
