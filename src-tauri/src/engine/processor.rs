use std::path::{Path, PathBuf};

use crate::{
    error::{Error, Result},
    transcriber::Segment,
};

use super::{
    chunk::{ChunkProcessor, segments_from_sherpa},
    sherpa::{parse_json, run_cmd},
};

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
