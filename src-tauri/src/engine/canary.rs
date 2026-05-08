use std::path::PathBuf;

use crate::{
    config::Config,
    engine::{
        chunk::run_single,
        processor::{Processor, resolve_variant},
        runtime,
        sherpa::find_binary,
    },
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
        [
            |dir: &std::path::Path| build_paths(dir, "encoder.int8.onnx", "decoder.int8.onnx"),
            |dir: &std::path::Path| build_paths(dir, "encoder.onnx", "decoder.onnx"),
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
    let bin = find_binary()?;
    let paths = resolve(&config.model)?;
    let lang = canary_lang(config);
    let lang_for_args = lang.clone();
    let processor = Processor {
        bin,
        build_args: Box::new(move |wav| {
            vec![
                format!("--canary-encoder={}", paths.encoder.display()),
                format!("--canary-decoder={}", paths.decoder.display()),
                format!("--tokens={}", paths.tokens.display()),
                format!("--canary-src-lang={lang_for_args}"),
                format!("--canary-tgt-lang={lang_for_args}"),
                "--canary-use-pnc=true".into(),
                format!("--num-threads={}", runtime::threads(config)),
                format!("--provider={}", runtime::provider(config).as_arg()),
                wav.display().to_string(),
            ]
        }),
        cancelled,
    };
    let (segs, rtf) = run_single(samples, audio_dur_sec, processor, on_progress)?;
    Ok((segs, lang, rtf))
}
