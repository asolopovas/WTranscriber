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
    joiner: PathBuf,
    tokens: PathBuf,
}

fn build_paths(dir: &std::path::Path, suffix: &str) -> Option<Paths> {
    let p = Paths {
        encoder: dir.join(format!("encoder{suffix}.onnx")),
        decoder: dir.join(format!("decoder{suffix}.onnx")),
        joiner: dir.join(format!("joiner{suffix}.onnx")),
        tokens: dir.join("tokens.txt"),
    };
    (p.encoder.exists() && p.decoder.exists() && p.joiner.exists() && p.tokens.exists())
        .then_some(p)
}

fn resolve(model_id: &str) -> Result<Paths> {
    resolve_variant(
        model_id,
        "transducer",
        &[|dir| build_paths(dir, ".int8"), |dir| build_paths(dir, "")],
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
        format!("--tokens={}", paths.tokens.display()),
        format!("--encoder={}", paths.encoder.display()),
        format!("--decoder={}", paths.decoder.display()),
        format!("--joiner={}", paths.joiner.display()),
        "--decoding-method=greedy_search".into(),
        "--model-type=nemo_transducer".into(),
    ];
    let (segs, rtf) = SubprocessSpec {
        model_args,
        config,
        strategy: ChunkStrategy::Single,
        cancelled,
    }
    .execute(samples, audio_dur_sec, on_progress)?;
    Ok((segs, "en".into(), rtf))
}
