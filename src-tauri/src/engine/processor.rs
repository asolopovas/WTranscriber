use std::path::{Path, PathBuf};

use crate::{
    config::Config,
    error::{Error, Result},
    transcriber::Segment,
};

use super::{
    chunk::{ChunkProcessor, run_chunked, run_single, segments_from_sherpa},
    runtime,
    sherpa::{find_binary, parse_json, run_cmd},
};

#[derive(Debug, Clone, Copy)]
pub enum ChunkStrategy {
    Single,
    Fixed(f64),
}

pub struct SubprocessSpec<'a> {
    pub model_args: Vec<String>,
    pub config: &'a Config,
    pub strategy: ChunkStrategy,
    pub cancelled: &'a dyn Fn() -> bool,
}

impl SubprocessSpec<'_> {
    pub fn execute(
        self,
        samples: &[f32],
        audio_dur_sec: f64,
        on_progress: &mut dyn FnMut(f64),
    ) -> Result<(Vec<Segment>, f64)> {
        let bin = find_binary()?;
        let Self {
            model_args,
            config,
            strategy,
            cancelled,
        } = self;

        let threads = runtime::threads(config);
        let provider = runtime::provider(config).as_arg();
        let processor = Processor {
            bin,
            build_args: Box::new(move |wav| {
                let mut a = model_args.clone();
                a.push(format!("--num-threads={threads}"));
                a.push(format!("--provider={provider}"));
                a.push(wav.display().to_string());
                a
            }),
            cancelled,
        };
        match strategy {
            ChunkStrategy::Single => run_single(samples, audio_dur_sec, processor, on_progress),
            ChunkStrategy::Fixed(sec) => {
                run_chunked(samples, audio_dur_sec, sec, processor, on_progress)
            }
        }
    }
}

pub type ArgsBuilder<'a> = Box<dyn Fn(&Path) -> Vec<String> + 'a>;

pub struct Processor<'a> {
    pub bin: PathBuf,
    pub build_args: ArgsBuilder<'a>,
    pub cancelled: &'a dyn Fn() -> bool,
}

impl ChunkProcessor for Processor<'_> {
    fn process(&mut self, wav: &Path, chunk_dur_sec: f64) -> Result<Vec<Segment>> {
        let args = (self.build_args)(wav);
        let (stdout, _, _) = run_cmd(&self.bin, &args, self.cancelled)?;
        Ok(parse_json(&stdout)
            .map(|r| segments_from_sherpa(&r, chunk_dur_sec))
            .unwrap_or_default())
    }

    fn is_cancelled(&self) -> bool {
        (self.cancelled)()
    }
}

pub fn resolve_variant<T>(
    model_id: &str,
    label: &str,
    variants: &[fn(&Path) -> Option<T>],
) -> Result<T> {
    let dir = crate::models::model_dir(model_id)?;
    for build in variants {
        if let Some(p) = build(&dir) {
            return Ok(p);
        }
    }
    Err(Error::Transcribe(format!(
        "{label} model files missing in {}",
        dir.display()
    )))
}
