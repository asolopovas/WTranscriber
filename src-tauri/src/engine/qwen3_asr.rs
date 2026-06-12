use std::path::PathBuf;

use crate::{
    config::Config,
    engine::processor::{ChunkStrategy, SubprocessSpec, resolve_variant},
    error::Result,
    transcriber::Segment,
};

#[derive(Debug, Clone)]
struct Paths {
    conv_frontend: PathBuf,
    encoder: PathBuf,
    decoder: PathBuf,
    tokenizer: PathBuf,
}

fn build_paths(dir: &std::path::Path, encoder: &str, decoder: &str) -> Option<Paths> {
    let p = Paths {
        conv_frontend: dir.join("conv_frontend.onnx"),
        encoder: dir.join(encoder),
        decoder: dir.join(decoder),
        tokenizer: dir.join("tokenizer"),
    };
    (p.conv_frontend.exists() && p.encoder.exists() && p.decoder.exists() && p.tokenizer.is_dir())
        .then_some(p)
}

fn resolve(model_id: &str) -> Result<Paths> {
    resolve_variant(
        model_id,
        "qwen3-asr",
        &[
            |dir| build_paths(dir, "encoder.int8.onnx", "decoder.int8.onnx"),
            |dir| build_paths(dir, "encoder.onnx", "decoder.onnx"),
        ],
    )
}

pub fn run(
    samples: &[f32],
    audio_dur_sec: f64,
    config: &Config,
    on_progress: &mut dyn FnMut(f64),
    cancelled: &dyn Fn() -> bool,
) -> Result<(Vec<Segment>, String, f64)> {
    let paths = resolve(&config.model)?;
    let model_args = vec![
        format!(
            "--qwen3-asr-conv-frontend={}",
            paths.conv_frontend.display()
        ),
        format!("--qwen3-asr-encoder={}", paths.encoder.display()),
        format!("--qwen3-asr-decoder={}", paths.decoder.display()),
        format!("--qwen3-asr-tokenizer={}", paths.tokenizer.display()),
    ];
    let (segs, rtf) = SubprocessSpec {
        model_args,
        config,
        strategy: ChunkStrategy::Single,
        cancelled,
    }
    .execute(samples, audio_dur_sec, on_progress)?;
    let lang = config.language.trim().to_lowercase();
    let detected = if lang == "auto" || lang.is_empty() {
        String::new()
    } else {
        lang
    };
    Ok((segs, detected, rtf))
}
