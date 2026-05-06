use std::path::{Path, PathBuf};

use crate::{
    config::Config,
    engine::{
        chunk::{ChunkProcessor, run_single, segments_from_sherpa},
        runtime,
        sherpa::{find_binary, parse_json, run_cmd},
    },
    error::{Error, Result},
    paths,
    transcriber::Segment,
};

#[derive(Debug, Clone)]
struct Paths {
    encoder: PathBuf,
    decoder: PathBuf,
    tokens: PathBuf,
}

fn resolve(model_id: &str) -> Result<Paths> {
    let dir = paths::models_dir()?.join(model_id);
    for (e, d) in [
        ("encoder.int8.onnx", "decoder.int8.onnx"),
        ("encoder.onnx", "decoder.onnx"),
    ] {
        let p = Paths {
            encoder: dir.join(e),
            decoder: dir.join(d),
            tokens: dir.join("tokens.txt"),
        };
        if p.encoder.exists() && p.decoder.exists() && p.tokens.exists() {
            return Ok(p);
        }
    }
    Err(Error::Transcribe(format!(
        "canary model files missing in {}",
        dir.display()
    )))
}

fn canary_lang(config: &Config) -> String {
    let lang = config.language.trim().to_lowercase();
    match lang.as_str() {
        "en" | "de" | "es" | "fr" => lang,
        _ => "en".into(),
    }
}

struct Processor<'a> {
    bin: PathBuf,
    paths: Paths,
    config: &'a Config,
    lang: String,
    cancelled: &'a dyn Fn() -> bool,
}

impl ChunkProcessor for Processor<'_> {
    fn process(&mut self, wav: &Path, chunk_dur_sec: f64) -> Result<Vec<Segment>> {
        let args = self.args(wav);
        let (stdout, _, _) = run_cmd(&self.bin, &args, self.cancelled)?;
        Ok(parse_json(&stdout)
            .map(|r| segments_from_sherpa(&r, chunk_dur_sec))
            .unwrap_or_default())
    }

    fn is_cancelled(&self) -> bool {
        (self.cancelled)()
    }
}

impl Processor<'_> {
    fn args(&self, wav: &Path) -> Vec<String> {
        vec![
            format!("--canary-encoder={}", self.paths.encoder.display()),
            format!("--canary-decoder={}", self.paths.decoder.display()),
            format!("--tokens={}", self.paths.tokens.display()),
            format!("--canary-src-lang={}", self.lang),
            format!("--canary-tgt-lang={}", self.lang),
            "--canary-use-pnc=true".into(),
            format!("--num-threads={}", runtime::threads(self.config)),
            format!("--provider={}", runtime::provider(self.config).as_arg()),
            wav.display().to_string(),
        ]
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
    let processor = Processor {
        bin,
        paths,
        config,
        lang: lang.clone(),
        cancelled,
    };
    let (segs, rtf) = run_single(samples, audio_dur_sec, processor, on_progress)?;
    Ok((segs, lang, rtf))
}
