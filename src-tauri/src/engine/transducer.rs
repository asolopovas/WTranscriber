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

#[derive(Debug, Clone, Copy)]
pub enum Kind {
    Zipformer,
    Parakeet,
}

impl Kind {
    const fn model_type(self) -> Option<&'static str> {
        match self {
            Self::Zipformer => None,
            Self::Parakeet => Some("nemo_transducer"),
        }
    }
}

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
    kind: Kind,
    samples: &[f32],
    audio_dur_sec: f64,
    config: &Config,
    on_progress: &mut dyn FnMut(f64),
    cancelled: &dyn Fn() -> bool,
) -> Result<(Vec<Segment>, String, f64)> {
    let bin = find_binary()?;
    let paths = resolve(&config.model)?;
    let processor = Processor {
        bin,
        build_args: Box::new(move |wav| {
            let mut a = vec![
                format!("--tokens={}", paths.tokens.display()),
                format!("--encoder={}", paths.encoder.display()),
                format!("--decoder={}", paths.decoder.display()),
                format!("--joiner={}", paths.joiner.display()),
                format!("--num-threads={}", runtime::threads(config)),
                "--decoding-method=greedy_search".into(),
                format!("--provider={}", runtime::provider(config).as_arg()),
            ];
            if let Some(mt) = kind.model_type() {
                a.push(format!("--model-type={mt}"));
            }
            a.push(wav.display().to_string());
            a
        }),
        cancelled,
    };
    let (segs, rtf) = run_single(samples, audio_dur_sec, processor, on_progress)?;
    Ok((segs, "en".into(), rtf))
}
