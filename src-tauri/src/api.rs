pub use crate::{
    config::{Config, Device, DiarizerChoice, Engine},
    error::{Error, Result},
    models::{Family, FileProgress, ModelInfo, manager},
    namer::Suggestion,
    progress::{Phase, Sink},
    transcriber::{
        Transcript, Utterance, Word,
        cache::{self as transcript_cache},
        partial as transcript_partial, run as transcribe, run_with_sink as transcribe_with_sink,
    },
};

pub use crate::transcriber::Job;

#[must_use]
pub fn engine_for_model(id: &str) -> Option<Engine> {
    crate::models::by_id(id).and_then(crate::models::Entry::engine_kind)
}

#[must_use]
pub fn is_model_installed(id: &str) -> bool {
    let Some(entry) = crate::models::by_id(id) else {
        return false;
    };
    crate::models::paths_for(entry)
        .is_ok_and(|paths| !paths.is_empty() && paths.iter().all(|p| p.exists()))
}

/// Pick the best installed ASR model id (and its engine) for a detected language code.
///
/// Priority follows current benchmarks:
///   `ru`            -> `GigaAM` v3 (Russian-specialised)
///   parakeet's 25   -> Parakeet TDT 0.6B v3 (token timestamps, fast)
///   everything else -> Whisper-turbo (multilingual fallback)
/// Falls through to the next candidate if a model is not installed.
#[must_use]
pub fn route_model_for_lang(lang: &str) -> Option<(String, Engine)> {
    let normalised = lang.trim().to_ascii_lowercase();
    let lang_code = normalised.split(['-', '_']).next().unwrap_or("");
    let candidates: &[&str] = match lang_code {
        "ru" => &[
            "gigaam-v3-ru",
            "parakeet-tdt-0.6b-v3-int8",
            "whisper-cpp-large-v3-turbo-q8",
        ],
        "bg" | "hr" | "cs" | "da" | "nl" | "en" | "et" | "fi" | "fr" | "de" | "el" | "hu"
        | "it" | "lv" | "lt" | "mt" | "pl" | "pt" | "ro" | "sk" | "sl" | "es" | "sv" | "uk" => {
            &["parakeet-tdt-0.6b-v3-int8", "whisper-cpp-large-v3-turbo-q8"]
        }
        _ => &["whisper-cpp-large-v3-turbo-q8"],
    };
    candidates
        .iter()
        .find(|id| is_model_installed(id))
        .and_then(|id| engine_for_model(id).map(|e| ((*id).to_owned(), e)))
}
