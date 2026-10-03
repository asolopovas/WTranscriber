use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

use serde::Deserialize;

use crate::{
    audio,
    config::{Config, DiarizerChoice},
    error::{Error, Result},
    paths,
    progress::{Phase, Sink},
};

use super::{Meta, Transcript, Word, cache, transcript};

pub const BACKEND: &str = "whisperx-community-1-v1";

pub fn enabled(config: &Config) -> bool {
    config.diarize && config.diarizer == DiarizerChoice::WhisperxCommunity1
}

#[derive(Deserialize)]
struct Output {
    words: Vec<Word>,
    language: String,
    duration_ms: u64,
    model: String,
    warnings: Vec<String>,
}

#[derive(Deserialize)]
struct Progress {
    phase: Phase,
    percent: f64,
}

struct Worker(Child);

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn python() -> Result<PathBuf> {
    if cfg!(any(target_os = "android", target_os = "ios")) {
        return Err(Error::Config(
            "WhisperX + Community-1 requires a desktop".into(),
        ));
    }
    let path = std::env::var_os("WT_QUALITY_PYTHON").map_or_else(
        || {
            paths::data_dir().map(|p| {
                p.join(if cfg!(windows) {
                    "quality/.venv/Scripts/python.exe"
                } else {
                    "quality/.venv/bin/python"
                })
            })
        },
        |p| Ok(PathBuf::from(p)),
    )?;
    if !path.is_file() {
        return Err(Error::Config(
            "WhisperX is not installed. Follow docs/quality-pipeline.md to install the local worker and authenticate with Hugging Face".into(),
        ));
    }
    Ok(path)
}

pub fn preflight() -> Result<()> {
    python().map(|_| ())
}

pub fn run(
    input: &Path,
    config: &Config,
    sink: &dyn Sink,
    previous: &Transcript,
) -> Result<Transcript> {
    let python = python()?;
    if sink.is_cancelled() {
        return Err(Error::Cancelled);
    }
    sink.phase(Phase::LoadingAudio);
    sink.set_diarize_backend(BACKEND);
    let wav = audio::ensure_cached_wav(input)?;
    let trim = audio::meta::load_checked(input)?.unwrap_or_default();
    let dir = tempfile::tempdir_in(paths::cache_dir()?)?;
    let script = dir.path().join("quality.py");
    let request = dir.path().join("request.json");
    let output = dir.path().join("output.json");
    let errors = dir.path().join("stderr.log");
    std::fs::write(&script, include_str!("../../python/quality.py"))?;
    std::fs::write(
        &request,
        serde_json::to_vec(&serde_json::json!({
            "wav": wav,
            "device": config.device.as_str(),
            "threads": config.threads,
            "language": config.language,
            "speakers": config.speakers,
            "start_ms": trim.trim_start_ms,
            "end_ms": trim.trim_end_ms,
            "previous": previous,
        }))?,
    )?;
    let mut worker = Worker(
        Command::new(python)
            .args([script.as_os_str(), request.as_os_str(), output.as_os_str()])
            .env("PYANNOTE_METRICS_ENABLED", "0")
            .env("HF_HUB_DISABLE_TELEMETRY", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(std::fs::File::create(&errors)?)
            .spawn()
            .map_err(|e| Error::Transcribe(format!("could not start WhisperX: {e}")))?,
    );
    wait_worker(&mut worker, &errors, sink)?;
    let output: Output = serde_json::from_slice(&std::fs::read(output)?)?;
    validate_words(
        &output.words,
        output.duration_ms,
        trim.trim_start_ms,
        trim.trim_end_ms,
    )?;
    for warning in &output.warnings {
        sink.warn(warning);
    }
    Ok(transcript::from_words(
        output.words,
        Meta {
            model: output.model,
            language: output.language,
            duration_ms: output.duration_ms,
            diarizer: Some(BACKEND.into()),
            device: Some(config.device.as_str().into()),
        },
    ))
}

fn wait_worker(worker: &mut Worker, errors: &Path, sink: &dyn Sink) -> Result<()> {
    let stdout = worker
        .0
        .stdout
        .take()
        .ok_or_else(|| Error::Transcribe("missing worker output".into()))?;
    let (send, recv) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout)
            .lines()
            .map_while(std::result::Result::ok)
        {
            if let Ok(progress) = serde_json::from_str::<Progress>(&line)
                && send.send(progress).is_err()
            {
                break;
            }
        }
    });
    let started = Instant::now();
    let mut phase = Phase::LoadingAudio;
    loop {
        if sink.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if started.elapsed() > Duration::from_secs(24 * 60 * 60) {
            return Err(Error::Transcribe(
                "WhisperX exceeded the 24-hour job limit".into(),
            ));
        }
        match recv.recv_timeout(Duration::from_millis(100)) {
            Ok(p) => {
                if phase != p.phase {
                    phase = p.phase;
                    sink.phase(phase);
                }
                sink.report_pct(phase, p.percent.clamp(0.0, 100.0));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                std::thread::park_timeout(Duration::from_millis(100));
            }
        }
        if let Some(status) = worker.0.try_wait()? {
            if !status.success() {
                let log = std::fs::read_to_string(errors).unwrap_or_default();
                let lines: Vec<_> = log.lines().collect();
                let tail = &lines[lines.len().saturating_sub(12)..];
                return Err(Error::Transcribe(format!(
                    "WhisperX failed: {}",
                    tail.join("\n")
                )));
            }
            break;
        }
    }
    Ok(())
}

fn validate_words(words: &[Word], duration: u64, start: u64, end: Option<u64>) -> Result<()> {
    let end = end.unwrap_or(duration).min(duration);
    for word in words {
        if word.start_ms < start
            || word.end_ms > end
            || word.start_ms > word.end_ms
            || !word.confidence.is_finite()
        {
            return Err(Error::Transcribe(
                "WhisperX returned invalid word timestamps".into(),
            ));
        }
    }
    Ok(())
}

pub fn finish(
    input: &Path,
    config: &Config,
    sink: &dyn Sink,
    segments: &[super::Segment],
    language: String,
    duration_ms: u64,
) -> Result<Transcript> {
    crate::engine::shutdown();
    let previous = transcript::build(
        segments,
        &[],
        Meta {
            model: config.model.clone(),
            language,
            duration_ms,
            ..Meta::default()
        },
    );
    let result = run(input, config, sink, &previous)?;
    if sink.is_cancelled() {
        return Err(Error::Cancelled);
    }
    sink.phase(Phase::Writing);
    store(input, config, &result)?;
    sink.phase(Phase::Done);
    Ok(result)
}

pub fn store(input: &Path, config: &Config, transcript: &Transcript) -> Result<()> {
    let trim = audio::meta::load_checked(input)?.unwrap_or_default();
    let params = cache::build_key_params(
        input,
        cache::KeyOptions {
            model: &transcript.model,
            language: &config.language,
            speakers: config.speakers.unwrap_or(0),
            no_diarize: false,
            trim_start_ms: trim.trim_start_ms,
            trim_end_ms: trim.trim_end_ms.unwrap_or(0),
            precise_word_timestamps: true,
            diarizer: BACKEND,
        },
    )?;
    let key = cache::compute_key(&params);
    super::saved::backup(input)?;
    cache::store(
        cache::Entry {
            key,
            source_path: params.source_path,
            source_name: input
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            model: transcript.model.clone(),
            language: transcript.language.clone(),
            speakers: config.speakers.unwrap_or(0),
            no_diarize: false,
            utterances: transcript.utterances.len(),
            duration_ms: transcript.duration_ms,
            created_at: chrono::Utc::now(),
            size_bytes: 0,
        },
        transcript,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn waits_for_exit_after_progress_pipe_closes() {
        let dir = tempfile::tempdir().unwrap();
        let errors = dir.path().join("stderr.log");
        let mut worker = Worker(
            Command::new("sh")
                .args(["-c", "exec 1>&-; read ignored; exit 0"])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(std::fs::File::create(&errors).unwrap())
                .spawn()
                .unwrap(),
        );
        let input = worker.0.stdin.take().unwrap();
        let feeder = std::thread::spawn(move || {
            std::thread::park_timeout(Duration::from_millis(200));
            drop(input);
        });
        wait_worker(&mut worker, &errors, &crate::progress::NoopSink).unwrap();
        feeder.join().unwrap();
    }

    #[test]
    fn rejects_words_outside_source_trim() {
        let word = Word {
            text: "football".into(),
            start_ms: 381_100,
            end_ms: 382_400,
            speaker: Some("SPEAKER_02".into()),
            confidence: 0.9,
        };
        assert!(
            validate_words(std::slice::from_ref(&word), 400_000, 33_352, Some(390_000)).is_ok()
        );
        assert!(validate_words(std::slice::from_ref(&word), 382_000, 33_352, None).is_err());
        assert!(validate_words(&[word], 400_000, 382_000, None).is_err());
    }
}
