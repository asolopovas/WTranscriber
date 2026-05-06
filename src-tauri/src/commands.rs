#![allow(
    clippy::needless_pass_by_value,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::{runtime::Handle, task::JoinHandle};

use crate::{
    audio,
    browser::{self, DirListing},
    config::Config,
    error::{Error, Result},
    logfile,
    models::{self, FileProgress, ModelInfo, ModelStatus},
    namer::{self, Suggestion},
    progress::{self, Phase, Sink, Smoother},
    transcriber::{self, Job, Transcript, export::Format as ExportFormat},
};

#[tauri::command]
pub const fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[tauri::command]
pub fn load_config() -> Result<Config> {
    Config::load()
}

#[tauri::command]
pub fn save_config(config: Config) -> Result<()> {
    config.save()
}

#[tauri::command]
pub fn list_models() -> Result<Vec<ModelInfo>> {
    models::manager().list()
}

#[tauri::command]
pub fn model_status(id: String) -> Result<ModelStatus> {
    models::manager().status(&id)
}

#[tauri::command]
pub async fn install_model(app: AppHandle, id: String) -> Result<()> {
    let mut on_progress = |p: FileProgress| {
        let _ = app.emit("model:progress", &p);
    };
    logfile::info(&format!("install_model {id} starting"));
    let result = models::manager().install(&id, &mut on_progress).await;
    match &result {
        Ok(()) => logfile::info(&format!("install_model {id} ok")),
        Err(e) => logfile::error(&format!("install_model {id}: {e}")),
    }
    let _ = app.emit(
        if result.is_ok() {
            "model:done"
        } else {
            "model:error"
        },
        &id,
    );
    result
}

#[tauri::command]
pub fn probe_audio(path: PathBuf) -> Option<u64> {
    audio::probe_duration_ms(&path)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    path: String,
    phase: Phase,
    display_pct: f64,
    elapsed_sec: f64,
    eta_sec: f64,
}

struct TranscribeSink {
    app: AppHandle,
    handle: Handle,
    file_path: String,
    smoother: Arc<Mutex<Smoother>>,
    current_phase: Mutex<Phase>,
    ticker_cancel: Arc<AtomicBool>,
    ticker_handle: Mutex<Option<JoinHandle<()>>>,
}

impl TranscribeSink {
    fn new(app: AppHandle, handle: Handle, file_path: String, audio_dur_sec: f64, initial_rtf: f64) -> Self {
        Self {
            app,
            handle,
            file_path,
            smoother: Arc::new(Mutex::new(Smoother::new(audio_dur_sec, initial_rtf))),
            current_phase: Mutex::new(Phase::CacheCheck),
            ticker_cancel: Arc::new(AtomicBool::new(false)),
            ticker_handle: Mutex::new(None),
        }
    }

    fn emit(&self, phase: Phase, display_pct: f64, eta_sec: f64) {
        let elapsed_sec = self
            .smoother
            .lock()
            .map_or(0.0, |s| s.elapsed().as_secs_f64());
        let _ = self.app.emit(
            "transcribe:progress",
            &ProgressEvent {
                path: self.file_path.clone(),
                phase,
                display_pct,
                elapsed_sec,
                eta_sec,
            },
        );
    }

    fn start_ticker(&self) {
        let mut handle_lock = self.ticker_handle.lock().unwrap();
        if handle_lock.is_some() {
            return;
        }
        self.ticker_cancel.store(false, Ordering::SeqCst);
        let app = self.app.clone();
        let path = self.file_path.clone();
        let smoother = self.smoother.clone();
        let cancel = self.ticker_cancel.clone();
        let join = self.handle.spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(500));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                if cancel.load(Ordering::SeqCst) {
                    break;
                }
                let (display_pct, eta_sec, elapsed_sec) = {
                    let Ok(mut s) = smoother.lock() else { break };
                    let (d, e) = s.snapshot();
                    (d, e, s.elapsed().as_secs_f64())
                };
                let _ = app.emit(
                    "transcribe:progress",
                    &ProgressEvent {
                        path: path.clone(),
                        phase: Phase::Transcribing,
                        display_pct,
                        elapsed_sec,
                        eta_sec,
                    },
                );
            }
        });
        *handle_lock = Some(join);
    }

    fn stop_ticker(&self) {
        self.ticker_cancel.store(true, Ordering::SeqCst);
        let taken = self.ticker_handle.lock().unwrap().take();
        if let Some(h) = taken {
            h.abort();
        }
    }
}

impl Drop for TranscribeSink {
    fn drop(&mut self) {
        self.stop_ticker();
    }
}

impl Sink for TranscribeSink {
    fn phase(&self, phase: Phase) {
        if let Ok(mut cur) = self.current_phase.lock() {
            *cur = phase;
        }
        match phase {
            Phase::Transcribing => self.start_ticker(),
            _ => self.stop_ticker(),
        }
        self.emit(phase, 0.0, 0.0);
    }

    fn report_pct(&self, phase: Phase, pct: f64) {
        match phase {
            Phase::Transcribing => {
                if let Ok(mut s) = self.smoother.lock() {
                    s.report(pct as i32);
                }
            }
            Phase::Diarizing => {
                self.emit(phase, pct, 0.0);
            }
            _ => {}
        }
    }
}

#[tauri::command]
pub async fn transcribe_file(
    app: AppHandle,
    input: PathBuf,
    config: Config,
) -> Result<Transcript> {
    let label = format!(
        "transcribe {} model={} engine={:?} lang={} device={:?}",
        input.display(),
        config.model,
        config.engine,
        config.language,
        config.device,
    );
    logfile::process_start(&label);
    let audio_dur_ms = audio::probe_duration_ms(&input).unwrap_or(0);
    let audio_dur_sec = if audio_dur_ms > 0 {
        audio_dur_ms as f64 / 1000.0
    } else {
        1.0
    };
    let device_label = format!("{:?}", config.device).to_lowercase();
    let initial_rtf = progress::load_rtf(&config.model, &device_label);
    let sink = Arc::new(TranscribeSink::new(
        app,
        Handle::current(),
        input.to_string_lossy().into_owned(),
        audio_dur_sec,
        initial_rtf,
    ));
    let job = Job { input, config };
    match transcriber::run_with_sink(&job, sink.clone()).await {
        Ok(t) => {
            logfile::process_end(
                &label,
                "ok",
                &format!(
                    "{} utterances, {} ms, {} speakers",
                    t.utterances.len(),
                    t.duration_ms,
                    t.speakers_detected
                ),
            );
            sink.emit(Phase::Done, 100.0, 0.0);
            Ok(t)
        }
        Err(e) => {
            logfile::error(&format!("{label}: {e}"));
            logfile::process_end(&label, "failed", &e.to_string());
            Err(e)
        }
    }
}

#[tauri::command]
pub fn log_path() -> Result<String> {
    Ok(logfile::log_path()?.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn log_tail(max_bytes: Option<u64>) -> String {
    logfile::read_tail(max_bytes.unwrap_or(256 * 1024))
}

#[tauri::command]
pub fn log_clear() -> Result<()> {
    logfile::clear()
}

#[tauri::command]
pub fn history_load(key: String) -> Result<Option<Transcript>> {
    transcriber::cache::load(&key)
}

#[tauri::command]
pub fn list_directory(path: Option<PathBuf>) -> Result<DirListing> {
    let p = path.unwrap_or_else(browser::home_dir);
    browser::list(&p)
}

#[tauri::command]
pub fn default_dir() -> PathBuf {
    browser::home_dir()
}

#[tauri::command]
pub fn add_to_workdir(source: PathBuf, workdir: PathBuf) -> Result<PathBuf> {
    if !source.is_file() {
        return Err(Error::Config(format!(
            "not a file: {}",
            source.display()
        )));
    }
    std::fs::create_dir_all(&workdir)?;
    let file_name = source
        .file_name()
        .ok_or_else(|| Error::Config("source has no file name".into()))?;
    let mut dst = workdir.join(file_name);
    if let Ok(src_canon) = std::fs::canonicalize(&source)
        && let Ok(dst_canon) = std::fs::canonicalize(&dst)
        && src_canon == dst_canon
    {
        return Ok(dst);
    }
    if dst.exists() {
        let stem = std::path::Path::new(file_name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file");
        let ext = std::path::Path::new(file_name)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        for n in 1..=999 {
            let candidate = if ext.is_empty() {
                workdir.join(format!("{stem} ({n})"))
            } else {
                workdir.join(format!("{stem} ({n}).{ext}"))
            };
            if !candidate.exists() {
                dst = candidate;
                break;
            }
        }
    }
    std::fs::copy(&source, &dst)?;
    logfile::info(&format!(
        "add_to_workdir {} -> {}",
        source.display(),
        dst.display()
    ));
    Ok(dst)
}

#[tauri::command]
pub fn rename_file(source: PathBuf, new_name: String) -> Result<PathBuf> {
    let trimmed = new_name.trim();
    if trimmed.is_empty() {
        return Err(Error::Config("new name is empty".into()));
    }
    if trimmed.contains(['/', '\\']) {
        return Err(Error::Config("new name must not contain path separators".into()));
    }
    let parent = source
        .parent()
        .ok_or_else(|| Error::Config("source has no parent directory".into()))?;
    let mut target_name = trimmed.to_owned();
    if std::path::Path::new(&target_name).extension().is_none()
        && let Some(ext) = source.extension().and_then(|e| e.to_str())
        && !ext.is_empty()
    {
        target_name.push('.');
        target_name.push_str(ext);
    }
    let dst = parent.join(&target_name);
    if dst == source {
        return Ok(dst);
    }
    if dst.exists() {
        return Err(Error::Config(format!(
            "destination already exists: {target_name}"
        )));
    }
    std::fs::rename(&source, &dst)?;
    logfile::info(&format!("rename {} -> {}", source.display(), dst.display()));
    Ok(dst)
}

#[tauri::command]
pub fn delete_file(path: PathBuf) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    std::fs::remove_file(&path)?;
    logfile::info(&format!("delete {}", path.display()));
    Ok(())
}

#[tauri::command]
pub fn export_transcript(
    transcript: Transcript,
    dest: PathBuf,
    format: ExportFormat,
) -> Result<PathBuf> {
    transcriber::export::write(&transcript, &dest, format)?;
    logfile::info(&format!("export {:?} -> {}", format, dest.display()));
    Ok(dest)
}

#[tauri::command]
pub async fn suggest_filename(transcript: Transcript) -> Result<Suggestion> {
    tokio::task::spawn_blocking(move || namer::suggest(&transcript, chrono::Local::now()))
        .await
        .map_err(|e| crate::error::Error::Transcribe(format!("task: {e}")))?
}
