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

#[derive(Debug, Clone)]
struct Paths {
    model: PathBuf,
    tokens: PathBuf,
}

fn resolve(model_id: &str) -> Result<Paths> {
    let dir = paths::models_dir()?.join(model_id);
    for m in ["model.int8.onnx", "model.onnx"] {
        let p = Paths {
            model: dir.join(m),
            tokens: dir.join("tokens.txt"),
        };
        if p.model.exists() && p.tokens.exists() {
            return Ok(p);
        }
    }
    Err(Error::Transcribe(format!(
        "nemo-ctc model files missing in {}",
        dir.display()
    )))
}

struct Processor<'a> {
    bin: PathBuf,
    paths: Paths,
    config: &'a Config,
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
        vec![
            format!("--nemo-ctc-model={}", self.paths.model.display()),
            format!("--tokens={}", self.paths.tokens.display()),
            format!("--num-threads={}", runtime::threads(self.config)),
            format!("--provider={}", runtime::provider(self.config).as_arg()),
            "--model-type=nemo_ctc".into(),
            wav.display().to_string(),
        ]
    }
}

pub fn run(
    samples: &[f32],
    audio_dur_sec: f64,
    config: &Config,
    on_progress: &mut dyn FnMut(f64),
) -> Result<(Vec<Segment>, String, f64)> {
    let bin = find_binary()?;
    let paths = resolve(&config.model)?;
    let processor = Processor { bin, paths, config };
    let (segs, rtf) = run_chunked(samples, audio_dur_sec, processor, on_progress)?;
    let lang = config.language.trim().to_lowercase();
    let detected = if lang == "auto" || lang.is_empty() {
        String::new()
    } else {
        lang
    };
    Ok((segs, detected, rtf))
}
