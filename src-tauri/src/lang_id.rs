//! Silero `LangID` (95-class) probe for routing audio to the right ASR.
//! Loads `silero-lang95-onnx/lang_classifier_95.onnx` once and reuses it.

#![allow(
    clippy::significant_drop_tightening,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

use std::sync::{Mutex, OnceLock};

use ort::{
    session::{Session, builder::GraphOptimizationLevel},
    value::TensorRef,
};

use crate::{
    audio,
    audio_toolkit::{
        constants::FRAME_SAMPLES,
        vad::{self as vad_kit, VadFrame, VoiceActivityDetector},
    },
    error::{Error, Result},
    logfile, models,
};

const MODEL_ID: &str = "silero-lang95-onnx";
const SAMPLE_RATE: u32 = 16_000;
use crate::constants::{
    LANG_ID_PROBE_SECONDS as PROBE_SECONDS, LANG_ID_VAD_SCAN_SECONDS as VAD_SCAN_SECONDS,
};

static SESSION: OnceLock<Mutex<Option<Session>>> = OnceLock::new();

fn slot() -> &'static Mutex<Option<Session>> {
    SESSION.get_or_init(|| Mutex::new(None))
}

fn model_path() -> Result<std::path::PathBuf> {
    let entry = models::by_id(MODEL_ID).ok_or_else(|| {
        Error::Config(format!("Silero LangID catalog entry `{MODEL_ID}` missing"))
    })?;
    let files = models::paths_for(entry)?;
    files
        .into_iter()
        .find(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("onnx"))
        })
        .ok_or_else(|| Error::Transcribe("Silero LangID has no .onnx file in catalog".into()))
}

#[must_use]
pub fn is_installed() -> bool {
    matches!(model_path(), Ok(p) if p.exists())
}

fn ensure_session() -> Result<()> {
    let mut guard = slot()
        .lock()
        .map_err(|e| Error::Transcribe(format!("lang-id session lock: {e}")))?;
    if guard.is_some() {
        return Ok(());
    }
    let path = model_path()?;
    if !path.exists() {
        return Err(Error::Transcribe(format!(
            "Silero LangID model missing: {} (install via `wt models install {MODEL_ID}`)",
            path.display(),
        )));
    }
    let session = Session::builder()
        .map_err(|e| Error::Transcribe(format!("lang-id session builder: {e}")))?
        .with_optimization_level(GraphOptimizationLevel::Level1)
        .map_err(|e| Error::Transcribe(format!("lang-id opt level: {e}")))?
        .commit_from_file(&path)
        .map_err(|e| Error::Transcribe(format!("lang-id load {}: {e}", path.display())))?;
    *guard = Some(session);
    Ok(())
}

/// Output-index → BCP-47-ish language code for `deepghs/silero-lang95-onnx`,
/// taken verbatim from the model's `lang_dict_95.json`. Regional suffixes
/// (`zh-CN`, `pa-IN`, `fy-NL`, etc.) are stripped to the base code so the
/// router can match its lookup tables.
const LANG_CODES_95: &[&str] = &[
    "fr", "zh", "ln", "fy", "hi", "ru", "yo", "da", "it", "hr", "si", "as", "lo", "uk", "ko", "az",
    "gu", "nn", "zh", "es", "lb", "ha", "id", "nl", "af", "pa", "oc", "ps", "et", "fa", "en", "sw",
    "fi", "sq", "hy", "tt", "ta", "mn", "kk", "ht", "ja", "ur", "yi", "sv", "ca", "vi", "am", "sl",
    "th", "hu", "pl", "tg", "uz", "sn", "mk", "ar", "la", "be", "km", "mg", "ne", "te", "or", "pt",
    "tl", "sk", "ky", "my", "pa", "no", "ro", "el", "ml", "mt", "bo", "de", "tk", "sr", "bg", "cs",
    "tr", "so", "bn", "ms", "sv", "zh", "mr", "su", "rw", "ba", "bs", "sd", "gl", "kn", "zh",
];

fn voiced_window(samples: &[f32], target_seconds: usize) -> Vec<f32> {
    let target = target_seconds.saturating_mul(SAMPLE_RATE as usize);
    let scan = VAD_SCAN_SECONDS.saturating_mul(SAMPLE_RATE as usize);
    let scan_slice = &samples[..samples.len().min(scan)];

    let Ok(vad_model_path) = vad_kit::model::model_path() else {
        let end = scan_slice.len().min(target);
        return scan_slice[..end].to_vec();
    };
    if !vad_kit::model::is_installed() {
        let end = scan_slice.len().min(target);
        return scan_slice[..end].to_vec();
    }

    let Ok(mut vad) = vad_kit::SileroVad::new(&vad_model_path, 0.5) else {
        let end = scan_slice.len().min(target);
        return scan_slice[..end].to_vec();
    };

    let mut collected: Vec<f32> = Vec::with_capacity(target);
    for frame in scan_slice.chunks_exact(FRAME_SAMPLES) {
        if let Ok(VadFrame::Speech(out)) = vad.push_frame(frame) {
            collected.extend_from_slice(out);
        }
        if collected.len() >= target {
            break;
        }
    }

    if collected.is_empty() {
        let end = scan_slice.len().min(target);
        return scan_slice[..end].to_vec();
    }
    if collected.len() > target {
        collected.truncate(target);
    }
    collected
}

/// Decode the audio file, take a voiced probe window via Silero VAD, run the
/// classifier, and return a lowercase language code (`"en"`, `"ru"`, …).
pub async fn detect_async(input: &std::path::Path) -> Result<String> {
    let _ = vad_kit::model::ensure().await;
    let input = input.to_path_buf();
    tokio::task::spawn_blocking(move || detect(&input))
        .await
        .map_err(|e| Error::Transcribe(format!("lang-id task: {e}")))?
}

pub fn detect(input: &std::path::Path) -> Result<String> {
    ensure_session()?;
    let samples = audio::decode::decode_to_pcm_f32(input, SAMPLE_RATE as i32)?;
    let probe = voiced_window(&samples, PROBE_SECONDS);
    if probe.is_empty() {
        return Err(Error::Transcribe(
            "lang-id probe: empty audio after decode".into(),
        ));
    }
    detect_samples(&probe)
}

fn detect_samples(samples: &[f32]) -> Result<String> {
    let mut guard = slot()
        .lock()
        .map_err(|e| Error::Transcribe(format!("lang-id session lock: {e}")))?;
    let session = guard
        .as_mut()
        .ok_or_else(|| Error::Transcribe("lang-id session not initialised".into()))?;

    let n = samples.len();
    let owned: Vec<f32> = samples.to_vec();
    let tensor = TensorRef::from_array_view((vec![1_i64, n as i64], &owned[..]))
        .map_err(|e| Error::Transcribe(format!("lang-id tensor: {e}")))?;

    let outputs = session
        .run(ort::inputs![tensor])
        .map_err(|e| Error::Transcribe(format!("lang-id run: {e}")))?;
    let (shape, data) = outputs[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| Error::Transcribe(format!("lang-id extract: {e}")))?;

    let class_dim = shape.last().copied().unwrap_or(0).max(0) as usize;
    if class_dim == 0 || data.is_empty() {
        return Err(Error::Transcribe(format!(
            "lang-id output empty (shape={shape:?})"
        )));
    }
    let logits = &data[data.len().saturating_sub(class_dim)..];
    let (idx, _) = logits
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .ok_or_else(|| Error::Transcribe("lang-id argmax over empty logits".into()))?;
    let code = LANG_CODES_95
        .get(idx)
        .copied()
        .ok_or_else(|| Error::Transcribe(format!("lang-id index {idx} out of table")))?;
    logfile::info(&format!(
        "silero-langid -> {code} (class {idx}/{class_dim}, {} samples)",
        samples.len()
    ));
    Ok(code.to_owned())
}
