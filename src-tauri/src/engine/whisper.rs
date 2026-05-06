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
struct WhisperPaths {
    encoder: PathBuf,
    decoder: PathBuf,
    tokens: PathBuf,
}

fn resolve_paths(model_id: &str) -> Result<WhisperPaths> {
    let dir = paths::models_dir()?.join(model_id);
    let candidates = [
        WhisperPaths {
            encoder: dir.join(format!("{model_id}-encoder.int8.onnx")),
            decoder: dir.join(format!("{model_id}-decoder.int8.onnx")),
            tokens: dir.join(format!("{model_id}-tokens.txt")),
        },
        WhisperPaths {
            encoder: dir.join("encoder.int8.onnx"),
            decoder: dir.join("decoder.int8.onnx"),
            tokens: dir.join("tokens.txt"),
        },
    ];
    for c in &candidates {
        if c.encoder.exists() && c.decoder.exists() && c.tokens.exists() {
            return Ok(c.clone());
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

struct Processor<'a> {
    bin: PathBuf,
    paths: WhisperPaths,
    config: &'a Config,
    language: Option<String>,
}

impl ChunkProcessor for Processor<'_> {
    fn process(&mut self, wav: &Path, chunk_dur_sec: f64) -> Result<Vec<Segment>> {
        let args = self.args(wav);
        let (stdout, _stderr, _) = run_cmd(&self.bin, &args)?;
        Ok(parse_json(&stdout)
            .map(|r| segments_from_sherpa(&r, chunk_dur_sec))
            .unwrap_or_default())
    }
}

impl Processor<'_> {
    fn args(&self, wav: &Path) -> Vec<String> {
        let mut a = vec![
            format!("--whisper-encoder={}", self.paths.encoder.display()),
            format!("--whisper-decoder={}", self.paths.decoder.display()),
            format!("--tokens={}", self.paths.tokens.display()),
            format!("--num-threads={}", runtime::threads(self.config)),
            format!("--provider={}", runtime::provider(self.config).as_arg()),
            "--model-type=whisper".into(),
        ];
        if let Some(lang) = self.language.as_deref() {
            a.push(format!("--whisper-language={lang}"));
        }
        a.push(wav.display().to_string());
        a
    }
}

pub fn run(
    samples: &[f32],
    audio_dur_sec: f64,
    config: &Config,
    on_progress: &mut dyn FnMut(f64),
) -> Result<(Vec<Segment>, String, f64)> {
    let bin = find_binary()?;
    let paths = resolve_paths(&config.model)?;
    let language = language_arg(config);

    let processor = Processor {
        bin,
        paths,
        config,
        language: language.clone(),
    };
    let (segs, rtf) = run_chunked(samples, audio_dur_sec, processor, on_progress)?;
    let detected = language.unwrap_or_else(|| config.language.clone());
    Ok((segs, detected, rtf))
}
