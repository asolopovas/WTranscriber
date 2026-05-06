use std::path::{Path, PathBuf};

use crate::{
    config::Config,
    engine::{
        chunk::{ChunkProcessor, run_chunked, segments_from_sherpa},
        runtime,
        sherpa::{find_binary, parse_json, run_cmd},
    },
    error::{Error, Result},
    paths,
    transcriber::Segment,
};

#[derive(Debug, Clone, Copy)]
pub enum Kind {
    Zipformer,
    Parakeet,
}

impl Kind {
    const fn name(self) -> &'static str {
        match self {
            Self::Zipformer => "zipformer",
            Self::Parakeet => "parakeet",
        }
    }

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

fn resolve(model_id: &str) -> Result<Paths> {
    let dir = paths::models_dir()?.join(model_id);
    for variant in [
        ("encoder.int8.onnx", "decoder.int8.onnx", "joiner.int8.onnx"),
        ("encoder.onnx", "decoder.onnx", "joiner.onnx"),
    ] {
        let p = Paths {
            encoder: dir.join(variant.0),
            decoder: dir.join(variant.1),
            joiner: dir.join(variant.2),
            tokens: dir.join("tokens.txt"),
        };
        if p.encoder.exists() && p.decoder.exists() && p.joiner.exists() && p.tokens.exists() {
            return Ok(p);
        }
    }
    Err(Error::Transcribe(format!(
        "transducer model files missing in {}",
        dir.display()
    )))
}

struct Processor<'a> {
    bin: PathBuf,
    paths: Paths,
    config: &'a Config,
    kind: Kind,
}

impl ChunkProcessor for Processor<'_> {
    fn process(&mut self, wav: &Path, chunk_dur_sec: f64) -> Result<Vec<Segment>> {
        let args = self.args(wav);
        let (stdout, _, _) = run_cmd(&self.bin, &args)?;
        Ok(parse_json(&stdout)
            .map(|r| segments_from_sherpa(&r, chunk_dur_sec))
            .unwrap_or_default())
    }
}

impl Processor<'_> {
    fn args(&self, wav: &Path) -> Vec<String> {
        let mut a = vec![
            format!("--tokens={}", self.paths.tokens.display()),
            format!("--encoder={}", self.paths.encoder.display()),
            format!("--decoder={}", self.paths.decoder.display()),
            format!("--joiner={}", self.paths.joiner.display()),
            format!("--num-threads={}", runtime::threads(self.config)),
            "--decoding-method=greedy_search".into(),
            format!("--provider={}", runtime::provider(self.config).as_arg()),
        ];
        if let Some(mt) = self.kind.model_type() {
            a.push(format!("--model-type={mt}"));
        }
        a.push(wav.display().to_string());
        a
    }
}

pub fn run(
    kind: Kind,
    samples: &[f32],
    audio_dur_sec: f64,
    config: &Config,
    on_progress: &mut dyn FnMut(f64),
) -> Result<(Vec<Segment>, String, f64)> {
    let bin = find_binary()?;
    let paths = resolve(&config.model)?;
    let processor = Processor { bin, paths, config, kind };
    let (segs, rtf) = run_chunked(samples, audio_dur_sec, processor, on_progress)?;
    let _ = kind.name();
    Ok((segs, "en".into(), rtf))
}
