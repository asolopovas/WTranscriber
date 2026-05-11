use std::path::PathBuf;

use crate::{
    config::Config,
    engine::processor::{ChunkStrategy, SubprocessSpec, resolve_variant},
    error::Result,
    transcriber::Segment,
};

#[derive(Debug, Clone)]
struct Paths {
    encoder: PathBuf,
    decoder: PathBuf,
    tokens: PathBuf,
}

fn resolve(model_id: &str) -> Result<Paths> {
    resolve_variant(
        model_id,
        "canary",
        &[
            |dir| build_paths(dir, "encoder.int8.onnx", "decoder.int8.onnx"),
            |dir| build_paths(dir, "encoder.onnx", "decoder.onnx"),
        ],
    )
}

fn build_paths(dir: &std::path::Path, encoder: &str, decoder: &str) -> Option<Paths> {
    let p = Paths {
        encoder: dir.join(encoder),
        decoder: dir.join(decoder),
        tokens: dir.join("tokens.txt"),
    };
    (p.encoder.exists() && p.decoder.exists() && p.tokens.exists()).then_some(p)
}

fn canary_lang(config: &Config) -> String {
    let lang = config.language.trim().to_lowercase();
    match lang.as_str() {
        "en" | "de" | "es" | "fr" => lang,
        _ => "en".into(),
    }
}

pub fn run(
    samples: &[f32],
    audio_dur_sec: f64,
    config: &Config,
    on_progress: &mut dyn FnMut(f64),
    cancelled: &dyn Fn() -> bool,
) -> Result<(Vec<Segment>, String, f64)> {
    let paths = resolve(&config.model)?;
    let lang = canary_lang(config);
    let model_args = vec![
        format!("--canary-encoder={}", paths.encoder.display()),
        format!("--canary-decoder={}", paths.decoder.display()),
        format!("--tokens={}", paths.tokens.display()),
        format!("--canary-src-lang={lang}"),
        format!("--canary-tgt-lang={lang}"),
        "--canary-use-pnc=true".into(),
    ];
    let (segs, rtf) = SubprocessSpec {
        model_args,
        config,
        strategy: ChunkStrategy::Single,
        cancelled,
    }
    .execute(samples, audio_dur_sec, on_progress)?;
    Ok((segs, lang, rtf))
}
