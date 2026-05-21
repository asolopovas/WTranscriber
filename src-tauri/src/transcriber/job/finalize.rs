use chrono::Utc;

use crate::{
    config::Config,
    diarizer::Segment as DiarSegment,
    error::Result,
    logfile,
    transcriber::{
        cache::{self, Entry as CacheEntry},
        partial,
        transcript::{self, Meta, Segment, Transcript},
    },
};

pub(super) struct BuildArgs<'a> {
    pub(super) segments: &'a [Segment],
    pub(super) diar_segs: &'a [DiarSegment],
    pub(super) diar_name: Option<String>,
    pub(super) config: &'a Config,
    pub(super) key: &'a str,
    pub(super) key_params: &'a cache::KeyParams,
    pub(super) speakers: u32,
    pub(super) language: String,
    pub(super) device_label: String,
    pub(super) duration_ms: u64,
}

pub(super) fn build_and_cache_transcript(args: BuildArgs<'_>) -> Result<Transcript> {
    let transcript = transcript::build(
        args.segments,
        args.diar_segs,
        Meta {
            model: args.config.model.clone(),
            language: args.language.clone(),
            duration_ms: args.duration_ms,
            diarizer: args.diar_name,
            device: Some(args.device_label),
        },
    );
    let entry = CacheEntry {
        key: args.key.to_string(),
        source_path: args.key_params.source_path.clone(),
        source_name: args
            .key_params
            .source_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        model: args.config.model.clone(),
        language: args.language,
        speakers: args.speakers,
        no_diarize: !args.config.diarize,
        utterances: transcript.utterances.len(),
        duration_ms: args.duration_ms,
        created_at: Utc::now(),
        size_bytes: 0,
    };
    cache::store(entry, &transcript)?;
    let _ = partial::clear(args.key);
    logfile::info(&format!(
        "cache stored: key={} utterances={} speakers_detected={}",
        args.key,
        transcript.utterances.len(),
        transcript.speakers_detected,
    ));
    Ok(transcript)
}
