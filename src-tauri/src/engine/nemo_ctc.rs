use std::path::PathBuf;

use crate::{
    config::Config,
    engine::processor::{ChunkStrategy, SubprocessSpec, resolve_variant},
    error::Result,
    transcriber::Segment,
};

#[derive(Debug, Clone)]
struct Paths {
    model: PathBuf,
    tokens: PathBuf,
}

fn build_paths(dir: &std::path::Path, model: &str) -> Option<Paths> {
    let p = Paths {
        model: dir.join(model),
        tokens: dir.join("tokens.txt"),
    };
    (p.model.exists() && p.tokens.exists()).then_some(p)
}

fn resolve(model_id: &str) -> Result<Paths> {
    resolve_variant(
        model_id,
        "nemo-ctc",
        &[
            |dir| build_paths(dir, "model.int8.onnx"),
            |dir| build_paths(dir, "model.onnx"),
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
        format!("--nemo-ctc-model={}", paths.model.display()),
        format!("--tokens={}", paths.tokens.display()),
        "--model-type=nemo_ctc".into(),
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
