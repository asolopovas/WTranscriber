use std::path::PathBuf;

use crate::{
    config::Config,
    engine::processor::{ChunkStrategy, SubprocessSpec},
    error::{Error, Result},
    transcriber::Segment,
};

#[derive(Debug, Clone)]
struct WhisperPaths {
    encoder: PathBuf,
    decoder: PathBuf,
    tokens: PathBuf,
}

fn build_paths(dir: &std::path::Path, prefix: &str) -> Option<WhisperPaths> {
    let p = WhisperPaths {
        encoder: dir.join(format!("{prefix}encoder.int8.onnx")),
        decoder: dir.join(format!("{prefix}decoder.int8.onnx")),
        tokens: dir.join(format!("{prefix}tokens.txt")),
    };
    (p.encoder.exists() && p.decoder.exists() && p.tokens.exists()).then_some(p)
}

fn resolve_paths(model_id: &str) -> Result<WhisperPaths> {
    let dir = crate::models::model_dir(model_id)?;
    let bare = model_id.strip_prefix("sherpa-whisper-").unwrap_or(model_id);
    for prefix in [&format!("{model_id}-"), &format!("{bare}-"), ""] {
        if let Some(p) = build_paths(&dir, prefix) {
            return Ok(p);
        }
    }
    Err(Error::Transcribe(format!(
        "whisper model files missing in {}",
        dir.display()
    )))
}

fn language_arg(config: &Config) -> Option<String> {
    let lang = config.language.trim();
    (!lang.is_empty() && lang != "auto").then(|| lang.to_owned())
}

pub fn run(
    samples: &[f32],
    audio_dur_sec: f64,
    config: &Config,
    on_progress: &mut dyn FnMut(f64),
    cancelled: &dyn Fn() -> bool,
) -> Result<(Vec<Segment>, String, f64)> {
    let paths = resolve_paths(&config.model)?;
    let language = language_arg(config);
    let mut model_args = vec![
        format!("--whisper-encoder={}", paths.encoder.display()),
        format!("--whisper-decoder={}", paths.decoder.display()),
        format!("--tokens={}", paths.tokens.display()),
        "--model-type=whisper".into(),
    ];
    if let Some(ref lang) = language {
        model_args.push(format!("--whisper-language={lang}"));
    }
    let (segs, rtf) = SubprocessSpec {
        model_args,
        config,
        strategy: ChunkStrategy::Whisper,
        cancelled,
    }
    .execute(samples, audio_dur_sec, on_progress)?;
    let detected = language.unwrap_or_else(|| config.language.clone());
    Ok((segs, detected, rtf))
}
