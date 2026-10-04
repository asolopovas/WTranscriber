use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

use serde::{Deserialize, Serialize};

use super::{Transcript, cache, export};
use crate::error::{Error, Result};

pub static EDIT_LOCK: Mutex<()> = Mutex::new(());

#[derive(Serialize, Deserialize)]
pub struct SavedTranscript {
    pub key: String,
    pub transcript: Transcript,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub undo: Vec<Transcript>,
}

pub fn path(source: &Path) -> PathBuf {
    let mut name = source.file_name().unwrap_or_default().to_owned();
    name.push(".transcript.json");
    source
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(".meta")
        .join(name)
}

pub fn text_path(source: &Path) -> PathBuf {
    let mut name = source.as_os_str().to_owned();
    name.push(".txt");
    PathBuf::from(name)
}

pub fn load(source: &Path) -> Result<Option<SavedTranscript>> {
    match std::fs::read(path(source)) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn stage(path: &Path, bytes: &[u8]) -> Result<tempfile::NamedTempFile> {
    crate::fs_utils::ensure_parent_dir(path)?;
    let mut file =
        tempfile::NamedTempFile::new_in(path.parent().unwrap_or_else(|| Path::new(".")))?;
    file.write_all(bytes)?;
    Ok(file)
}

pub fn store(key: &str, source: &Path, transcript: &Transcript) -> Result<()> {
    store_with_history(key, source, transcript, Vec::new())
}

pub fn store_edit(key: &str, source: &Path, transcript: &Transcript) -> Result<()> {
    let previous = load_for_key(key, Some(source))?
        .ok_or_else(|| Error::Config("transcript was not found".into()))?;
    if previous == *transcript {
        return Ok(());
    }
    let mut undo = load(source)?.map_or_else(Vec::new, |saved| saved.undo);
    undo.push(previous);
    store_with_history(key, source, transcript, undo)
}

pub fn can_undo(key: &str, source: &Path) -> Result<bool> {
    load_for_key(key, Some(source))?;
    Ok(load(source)?.is_some_and(|saved| !saved.undo.is_empty()))
}

pub fn undo_edit(key: &str, source: &Path) -> Result<Transcript> {
    load_for_key(key, Some(source))?;
    let mut saved =
        load(source)?.ok_or_else(|| Error::Config("there are no edits to undo".into()))?;
    let transcript = saved
        .undo
        .pop()
        .ok_or_else(|| Error::Config("there are no edits to undo".into()))?;
    store_with_history(key, source, &transcript, saved.undo)?;
    Ok(transcript)
}

fn store_with_history(
    key: &str,
    source: &Path,
    transcript: &Transcript,
    undo: Vec<Transcript>,
) -> Result<()> {
    let json = serde_json::to_vec_pretty(&SavedTranscript {
        key: key.into(),
        transcript: transcript.clone(),
        undo,
    })?;
    let mut text = Vec::new();
    export::write_to(transcript, &mut text, export::Format::Txt)?;
    let json_path = path(source);
    let text_path = text_path(source);
    let json_file = stage(&json_path, &json)?;
    let text_file = stage(&text_path, &text)?;
    let previous_text = match std::fs::read(&text_path) {
        Ok(bytes) => Some(bytes),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => return Err(err.into()),
    };
    text_file.persist(&text_path).map_err(|err| err.error)?;
    if let Err(err) = json_file.persist(&json_path) {
        if let Some(bytes) = previous_text {
            stage(&text_path, &bytes)?
                .persist(&text_path)
                .map_err(|err| err.error)?;
        } else {
            std::fs::remove_file(&text_path)?;
        }
        return Err(err.error.into());
    }
    Ok(())
}

pub fn source_for_key(key: &str, input: Option<&Path>) -> Result<PathBuf> {
    input
        .map(Path::to_path_buf)
        .or_else(|| {
            cache::list()
                .into_iter()
                .find(|entry| entry.key == key)
                .map(|entry| entry.source_path)
        })
        .ok_or_else(|| Error::Config("recording for transcript was not found".into()))
}

pub fn load_for_key(key: &str, input: Option<&Path>) -> Result<Option<Transcript>> {
    if let Ok(source) = source_for_key(key, input)
        && let Some(saved) = load(&source)?
    {
        if saved.key != key {
            return Err(Error::Config(
                "transcript changed; reopen it before editing".into(),
            ));
        }
        return Ok(Some(saved.transcript));
    }
    cache::load(key)
}

pub fn rename(source: &Path, destination: &Path) -> Result<()> {
    let moves = [
        (path(source), path(destination)),
        (text_path(source), text_path(destination)),
    ];
    for (from, to) in &moves {
        if from.exists() && to.exists() {
            return Err(Error::Config(format!(
                "transcript destination already exists: {}",
                to.display()
            )));
        }
    }
    let mut completed = Vec::new();
    for (from, to) in &moves {
        if from.exists() {
            crate::fs_utils::ensure_parent_dir(to)?;
            if let Err(error) = std::fs::rename(from, to) {
                for (from, to) in completed.iter().rev() {
                    std::fs::rename(to, from)?;
                }
                return Err(error.into());
            }
            completed.push((from, to));
        }
    }
    Ok(())
}

pub fn backup(source: &Path) -> Result<()> {
    let json = path(source);
    if !json.exists() {
        return Ok(());
    }
    let parent = json
        .parent()
        .ok_or_else(|| Error::Config("missing metadata directory".into()))?
        .join("backups");
    std::fs::create_dir_all(&parent)?;
    let dir = tempfile::Builder::new()
        .prefix("before-quality-")
        .tempdir_in(parent)?;
    std::fs::copy(&json, dir.path().join(json.file_name().unwrap_or_default()))?;
    let text = text_path(source);
    if text.exists() {
        std::fs::copy(&text, dir.path().join(text.file_name().unwrap_or_default()))?;
    }
    let _ = dir.keep();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{audio::meta, commands::diagnostics, transcriber::Utterance};

    fn transcript() -> Transcript {
        Transcript {
            model: "test".into(),
            language: "en".into(),
            duration_ms: 10_000,
            diarizer: Some("test".into()),
            device: None,
            speakers_detected: 1,
            utterances: vec![
                Utterance {
                    needs_review: false,
                    start_ms: 1000,
                    end_ms: 2000,
                    speaker: Some("SPEAKER_01".into()),
                    text: "First sentence.".into(),
                    language: None,
                },
                Utterance {
                    needs_review: false,
                    start_ms: 3000,
                    end_ms: 4000,
                    speaker: Some("SPEAKER_01".into()),
                    text: "Second sentence.".into(),
                    language: None,
                },
            ],
            words: Vec::new(),
        }
    }

    #[test]
    fn quality_backup_preserves_previous_json_and_text() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("recording.wav");
        store("old", &source, &transcript()).unwrap();
        let json = std::fs::read(path(&source)).unwrap();
        let text = std::fs::read(text_path(&source)).unwrap();
        backup(&source).unwrap();
        let backup_dir = std::fs::read_dir(dir.path().join(".meta/backups"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(
            std::fs::read(backup_dir.join("recording.wav.transcript.json")).unwrap(),
            json
        );
        assert_eq!(
            std::fs::read(backup_dir.join("recording.wav.txt")).unwrap(),
            text
        );
        let mut changed = transcript();
        changed.rename_speaker("SPEAKER_01", "SPEAKER_02");
        store("new", &source, &changed).unwrap();
        assert_eq!(
            std::fs::read(backup_dir.join("recording.wav.transcript.json")).unwrap(),
            json
        );
    }

    #[test]
    fn durable_results_and_edits_survive_cache_removal_and_preserve_trims() {
        let _guard = crate::paths::PATHS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = tempfile::tempdir().unwrap();
        crate::paths::init(
            dir.path().join("config"),
            dir.path().join("data"),
            dir.path().join("cache"),
        );
        let source = dir.path().join("recording.wav");
        std::fs::write(&source, b"audio").unwrap();
        meta::save(
            &source,
            &meta::AudioMeta {
                trim_start_ms: 1000,
                trim_end_ms: Some(9000),
                duration_ms: Some(10_000),
            },
        )
        .unwrap();
        cache::store(
            cache::Entry {
                key: "test-key".into(),
                source_path: source.clone(),
                source_name: "recording.wav".into(),
                model: "test".into(),
                language: "en".into(),
                speakers: 1,
                no_diarize: false,
                utterances: 2,
                duration_ms: 10_000,
                created_at: chrono::Utc::now(),
                size_bytes: 0,
            },
            &transcript(),
        )
        .unwrap();
        assert!(
            std::fs::read_to_string(text_path(&source))
                .unwrap()
                .contains("SPEAKER_01: First sentence.")
        );
        cache::clear_all().unwrap();
        let loaded = diagnostics::history_load("test-key".into(), Some(source.clone()))
            .unwrap()
            .unwrap();
        assert_eq!(loaded.utterances.len(), 2);
        let edited = diagnostics::update_transcript_text(
            "test-key".into(),
            source.clone(),
            0,
            "Corrected sentence.".into(),
        )
        .unwrap();
        assert_eq!(edited.words[0].text, "Corrected sentence.");
        let renamed = diagnostics::rename_speaker(
            "test-key".into(),
            "SPEAKER_01".into(),
            "Alice".into(),
            Some(source.clone()),
        )
        .unwrap();
        assert!(
            renamed
                .utterances
                .iter()
                .all(|u| u.speaker.as_deref() == Some("Alice"))
        );
        assert_eq!(renamed.words[0].speaker.as_deref(), Some("Alice"));
        let saved = load(&source).unwrap().unwrap();
        assert_eq!(saved.transcript.utterances[0].text, "Corrected sentence.");
        let text = std::fs::read_to_string(text_path(&source)).unwrap();
        assert!(text.contains("Alice: Corrected sentence."));
        assert!(text.contains("Alice: Second sentence."));
        assert!(!text.contains("SPEAKER_01"));
        assert_segment_corrections(&source);
        assert_eq!(meta::load(&source).unwrap().trim_start_ms, 1000);
        assert_eq!(meta::load(&source).unwrap().trim_end_ms, Some(9000));
        let listing = crate::browser::list(dir.path()).unwrap();
        assert_eq!(
            listing
                .entries
                .iter()
                .find(|e| e.is_audio)
                .unwrap()
                .cache_key
                .as_deref(),
            Some("test-key")
        );
        crate::paths::clear_test_overrides();
    }

    fn assert_segment_corrections(source: &Path) {
        let changed = diagnostics::set_transcript_speaker(
            "test-key".into(),
            source.to_path_buf(),
            0,
            "Bob".into(),
        )
        .unwrap();
        assert_eq!(changed.utterances[0].speaker.as_deref(), Some("Bob"));
        assert_eq!(changed.utterances[1].speaker.as_deref(), Some("Alice"));
        assert_eq!(changed.words[0].speaker.as_deref(), Some("Bob"));
        assert_eq!(changed.speakers_detected, 2);
        let replaced = diagnostics::replace_transcript_text(
            "test-key".into(),
            source.to_path_buf(),
            "sentence".into(),
            "phrase".into(),
        )
        .unwrap();
        assert_eq!(replaced.utterances[0].start_ms, 1000);
        assert_eq!(replaced.utterances[0].end_ms, 2000);
        let text = std::fs::read_to_string(text_path(source)).unwrap();
        assert!(text.contains("Bob: Corrected phrase."));
        assert!(text.contains("Alice: Second phrase."));
        assert_eq!(
            load(source).unwrap().unwrap().transcript.words[0].text,
            "Corrected phrase."
        );
        assert!(
            diagnostics::replace_transcript_text(
                "test-key".into(),
                source.to_path_buf(),
                String::new(),
                "bad".into()
            )
            .is_err()
        );
        assert!(
            diagnostics::set_transcript_speaker(
                "test-key".into(),
                source.to_path_buf(),
                99,
                "Bob".into()
            )
            .is_err()
        );
        assert!(
            diagnostics::set_transcript_speaker(
                "test-key".into(),
                source.to_path_buf(),
                0,
                " ".into()
            )
            .is_err()
        );
    }

    #[test]
    fn undo_restores_every_edit_and_survives_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("audio.wav");
        let original = transcript();
        store("key", &source, &original).unwrap();
        let mut states = vec![original.clone()];
        let mut edited = original;
        edited.edit_utterance(0, "Correction.").unwrap();
        store_edit("key", &source, &edited).unwrap();
        states.push(edited.clone());
        edited.set_utterance_speaker(0, "Alice").unwrap();
        store_edit("key", &source, &edited).unwrap();
        states.push(edited.clone());
        edited.rename_speaker("SPEAKER_01", "Bob");
        store_edit("key", &source, &edited).unwrap();
        states.push(edited.clone());
        edited.replace_text("sentence", "phrase").unwrap();
        store_edit("key", &source, &edited).unwrap();
        states.push(edited.clone());
        edited.mark_review(0, true).unwrap();
        store_edit("key", &source, &edited).unwrap();
        states.push(edited.clone());
        let recognised = super::super::transcript_for_retry(
            &[super::super::Segment {
                text: "Retry result.".into(),
                start_ms: 0,
                end_ms: 500,
                tokens: Vec::new(),
            }],
            &edited.utterances[0],
        )
        .unwrap();
        edited.apply_recognition(0, recognised).unwrap();
        store_edit("key", &source, &edited).unwrap();
        states.push(edited.clone());
        edited.delete_utterance(0).unwrap();
        store_edit("key", &source, &edited).unwrap();
        store_edit("key", &source, &edited).unwrap();
        assert_eq!(load(&source).unwrap().unwrap().undo.len(), states.len());
        for expected in states.into_iter().rev() {
            assert!(can_undo("key", &source).unwrap());
            let restored = undo_edit("key", &source).unwrap();
            assert_eq!(restored, expected);
            assert_eq!(load(&source).unwrap().unwrap().transcript, expected);
            let mut text = Vec::new();
            export::write_to(&expected, &mut text, export::Format::Txt).unwrap();
            assert_eq!(std::fs::read(text_path(&source)).unwrap(), text);
        }
        assert!(!can_undo("key", &source).unwrap());
        assert!(undo_edit("key", &source).is_err());
        assert!(undo_edit("stale-key", &source).is_err());
    }

    #[test]
    fn failed_undo_keeps_current_transcript_and_history() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("audio.wav");
        store("key", &source, &transcript()).unwrap();
        let mut edited = transcript();
        edited.delete_utterance(0).unwrap();
        store_edit("key", &source, &edited).unwrap();
        let before = std::fs::read(path(&source)).unwrap();
        std::fs::remove_file(text_path(&source)).unwrap();
        std::fs::create_dir(text_path(&source)).unwrap();
        assert!(undo_edit("key", &source).is_err());
        assert_eq!(std::fs::read(path(&source)).unwrap(), before);
        assert!(can_undo("key", &source).unwrap());
    }

    #[test]
    fn failed_text_export_leaves_saved_json_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("audio.wav");
        store("key", &source, &transcript()).unwrap();
        let before = std::fs::read(path(&source)).unwrap();
        std::fs::remove_file(text_path(&source)).unwrap();
        std::fs::create_dir(text_path(&source)).unwrap();
        let mut edited = transcript();
        edited.edit_utterance(0, "changed").unwrap();
        assert!(store("key", &source, &edited).is_err());
        assert_eq!(std::fs::read(path(&source)).unwrap(), before);
    }

    #[test]
    fn renaming_recording_moves_json_and_text() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("audio.wav");
        let destination = dir.path().join("renamed.wav");
        store("key", &source, &transcript()).unwrap();
        rename(&source, &destination).unwrap();
        assert!(load(&source).unwrap().is_none());
        assert!(load(&destination).unwrap().is_some());
        assert!(!text_path(&source).exists());
        assert!(text_path(&destination).exists());
    }

    #[test]
    fn segment_edit_keeps_other_segments_and_rejects_invalid_index() {
        let mut value = transcript();
        value.edit_utterance(0, "Correction.").unwrap();
        assert_eq!(value.utterances[0].start_ms, 1000);
        assert_eq!(value.utterances[0].end_ms, 2000);
        assert_eq!(value.utterances[1].text, "Second sentence.");
        assert!(value.edit_utterance(99, "bad").is_err());
    }
}
