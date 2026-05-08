#![allow(
    clippy::needless_pass_by_value,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, LazyLock, Mutex,
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
    models::{self, Family, FileProgress, ModelInfo, ModelStatus},
    namer::{self, Suggestion},
    paths,
    progress::{self, DiarizeSmoother, Phase, Sink, Smoother},
    transcriber::{self, Job, Transcript, export::Format as ExportFormat},
};

static TRANSCRIBE_CANCELS: LazyLock<Mutex<HashMap<String, Arc<AtomicBool>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[tauri::command]
pub const fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[derive(Serialize)]
pub struct SystemInfo {
    pub os: &'static str,
    pub arch: &'static str,
    pub cpu_threads: u32,
    pub is_mobile: bool,
    pub cuda_available: bool,
    pub nnapi_available: bool,
    pub app_version: &'static str,
    pub workdir: Option<String>,
    pub models_dir: Option<String>,
    pub cache_dir: Option<String>,
    pub config_dir: Option<String>,
    pub total_memory_bytes: u64,
}

#[tauri::command]
pub fn system_info() -> SystemInfo {
    let os = std::env::consts::OS;
    let is_mobile = matches!(os, "android" | "ios");
    let cpu_threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get) as u32;
    let cuda_available = !is_mobile && cfg!(feature = "cuda");
    let nnapi_available = os == "android";
    SystemInfo {
        os,
        arch: std::env::consts::ARCH,
        cpu_threads,
        is_mobile,
        cuda_available,
        nnapi_available,
        app_version: env!("CARGO_PKG_VERSION"),
        workdir: paths::default_workdir_override().map(|p| p.display().to_string()),
        models_dir: paths::models_dir().ok().map(|p| p.display().to_string()),
        cache_dir: paths::cache_dir().ok().map(|p| p.display().to_string()),
        config_dir: paths::config_file()
            .ok()
            .and_then(|p| p.parent().map(|d| d.display().to_string())),
        total_memory_bytes: read_total_memory(),
    }
}

fn read_total_memory() -> u64 {
    if let Ok(s) = std::fs::read_to_string("/proc/meminfo") {
        for line in s.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                let kb: u64 = rest
                    .split_whitespace()
                    .next()
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0);
                return kb * 1024;
            }
        }
    }
    0
}

#[tauri::command]
pub fn load_config() -> Result<Config> {
    Config::load()
}

#[tauri::command]
pub fn save_config(mut config: Config) -> Result<()> {
    sync_engine(&mut config);
    config.save()
}

fn sync_engine(config: &mut Config) {
    if let Some(m) = models::by_id(&config.model)
        && let Ok(e) = m.engine.parse::<crate::config::Engine>()
    {
        config.engine = e;
    }
}

#[tauri::command]
pub fn list_models() -> Result<Vec<ModelInfo>> {
    models::manager().list()
}

#[tauri::command]
pub fn essential_models() -> Vec<String> {
    crate::essential_model_ids()
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
pub fn delete_model(id: String) -> Result<()> {
    let Some(entry) = models::by_id(&id) else {
        return Err(Error::Config(format!("unknown model id {id}")));
    };
    for p in models::paths_for(entry)? {
        if p.exists() {
            std::fs::remove_file(&p).ok();
        }
    }
    let dir = models::model_dir(&id)?;
    if dir.exists() && std::fs::read_dir(&dir).is_ok_and(|r| r.count() == 0) {
        std::fs::remove_dir(&dir).ok();
    }
    logfile::info(&format!("delete_model {id} ok"));
    Ok(())
}

#[tauri::command]
pub fn probe_audio(path: PathBuf) -> Option<u64> {
    audio::probe_duration_ms(&path)
}

#[tauri::command]
pub fn audio_waveform(path: PathBuf, bins: usize) -> Result<Vec<f32>> {
    audio::waveform_peaks(&path, bins)
}

#[tauri::command]
pub fn load_audio_meta(path: PathBuf) -> audio::AudioMeta {
    audio::meta::load(&path).unwrap_or_default()
}

#[tauri::command]
pub fn save_audio_meta(path: PathBuf, meta: audio::AudioMeta) -> Result<()> {
    audio::meta::save(&path, &meta)
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
    diarize: Arc<Mutex<Option<DiarizeSmoother>>>,
    current_phase: Arc<Mutex<Phase>>,
    ticker_cancel: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    ticker_handle: Mutex<Option<JoinHandle<()>>>,
}

impl TranscribeSink {
    fn new(
        app: AppHandle,
        handle: Handle,
        file_path: String,
        audio_dur_sec: f64,
        initial_rtf: f64,
        cancel: Arc<AtomicBool>,
    ) -> Self {
        Self {
            app,
            handle,
            file_path,
            smoother: Arc::new(Mutex::new(Smoother::new(audio_dur_sec, initial_rtf))),
            diarize: Arc::new(Mutex::new(None)),
            current_phase: Arc::new(Mutex::new(Phase::CacheCheck)),
            ticker_cancel: Arc::new(AtomicBool::new(false)),
            cancel,
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
        let diarize = self.diarize.clone();
        let phase_lock = self.current_phase.clone();
        let cancel = self.ticker_cancel.clone();
        let join = self.handle.spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(500));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                if cancel.load(Ordering::SeqCst) {
                    break;
                }
                let phase = phase_lock.lock().ok().map_or(Phase::Transcribing, |g| *g);
                let elapsed_sec = smoother.lock().map_or(0.0, |s| s.elapsed().as_secs_f64());
                let snap = match phase {
                    Phase::Transcribing => smoother.lock().ok().map(|mut s| s.snapshot()),
                    Phase::Diarizing => diarize
                        .lock()
                        .ok()
                        .and_then(|mut g| g.as_mut().map(DiarizeSmoother::snapshot)),
                    _ => None,
                };
                let Some((display_pct, eta_sec)) = snap else {
                    continue;
                };
                let _ = app.emit(
                    "transcribe:progress",
                    &ProgressEvent {
                        path: path.clone(),
                        phase,
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
        let prev = self
            .current_phase
            .lock()
            .ok()
            .map(|mut g| std::mem::replace(&mut *g, phase));
        if matches!(phase, Phase::Diarizing)
            && let Ok(mut g) = self.diarize.lock()
            && g.is_none()
        {
            *g = Some(DiarizeSmoother::new());
        }
        match phase {
            Phase::Transcribing | Phase::Diarizing => self.start_ticker(),
            _ => self.stop_ticker(),
        }
        if prev.is_some_and(|p| p == phase) {
            self.emit(phase, 0.0, 0.0);
            return;
        }
        let elapsed = self
            .smoother
            .lock()
            .map_or(0.0, |s| s.elapsed().as_secs_f64());
        logfile::info(&format!("phase: {phase:?} (t+{elapsed:.1}s)"));
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
                if let Ok(mut g) = self.diarize.lock() {
                    if g.is_none() {
                        *g = Some(DiarizeSmoother::new());
                    }
                    if let Some(d) = g.as_mut() {
                        d.report(pct);
                    }
                }
            }
            _ => {}
        }
    }

    fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
}

#[tauri::command]
pub async fn transcribe_file(
    app: AppHandle,
    input: PathBuf,
    mut config: Config,
) -> Result<Transcript> {
    sync_engine(&mut config);
    validate_transcription_model(&config)?;
    let label = format!("transcribe {}", input.display());
    logfile::process_start(&label);
    log_preflight(&input, &config);
    let display_name = input
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("audio")
        .to_string();
    crate::android_start_transcription_service(&format!("Transcribing {display_name}"));
    let audio_dur_ms = audio::probe_duration_ms(&input).unwrap_or(0);
    let audio_dur_sec = if audio_dur_ms > 0 {
        audio_dur_ms as f64 / 1000.0
    } else {
        1.0
    };
    let device_label = format!("{:?}", config.device).to_lowercase();
    let initial_rtf = progress::load_rtf(&config.model, &device_label);
    let input_key = input.to_string_lossy().into_owned();
    let cancel = Arc::new(AtomicBool::new(false));
    if let Ok(mut cancels) = TRANSCRIBE_CANCELS.lock() {
        cancels.insert(input_key.clone(), cancel.clone());
    }
    let sink = Arc::new(TranscribeSink::new(
        app,
        Handle::current(),
        input_key.clone(),
        audio_dur_sec,
        initial_rtf,
        cancel,
    ));
    let job = Job { input, config };
    let result = match transcriber::run_with_sink(&job, sink.clone()).await {
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
            crate::android_notify_transcription_done(
                "Transcription finished",
                &format!("{display_name}: {} utterances", t.utterances.len()),
                true,
            );
            Ok(t)
        }
        Err(Error::Cancelled) => {
            logfile::process_end(&label, "cancelled", "user cancelled");
            Err(Error::Cancelled)
        }
        Err(e) => {
            logfile::error(&format!("{label}: {e}"));
            logfile::process_end(&label, "failed", &e.to_string());
            crate::android_notify_transcription_done(
                "Transcription failed",
                &format!("{display_name}: {e}"),
                false,
            );
            Err(e)
        }
    };
    crate::android_stop_transcription_service();
    if let Ok(mut cancels) = TRANSCRIBE_CANCELS.lock() {
        cancels.remove(&input_key);
    }
    result
}

fn log_preflight(input: &Path, config: &Config) {
    use std::fmt::Write as _;
    let mut buf = String::new();
    let _ = writeln!(buf, "Settings:");
    let _ = writeln!(buf, "  Engine    : {}", config.engine.as_str());
    let model_path = paths::models_dir()
        .map(|p| p.join(&config.model))
        .ok()
        .filter(|p| p.exists())
        .map_or_else(
            || config.model.clone(),
            |p| format!("{} ({})", config.model, p.display()),
        );
    let _ = writeln!(buf, "  Model     : {model_path}");
    let device_label = match config.device {
        crate::config::Device::Cuda => "GPU CUDA",
        crate::config::Device::Cpu => "CPU",
    };
    let _ = writeln!(buf, "  Device    : {device_label}");
    let _ = writeln!(buf, "  Language  : {}", config.language);
    let effective_threads = crate::engine::threads(config);
    if effective_threads == config.threads {
        let _ = writeln!(buf, "  Threads   : {}", config.threads);
    } else {
        let _ = writeln!(
            buf,
            "  Threads   : {effective_threads} (capped from {} for {device_label})",
            config.threads,
        );
    }
    if config.diarize {
        let _ = writeln!(
            buf,
            "  Diarizer  : {} (speakers={})",
            config.diarizer.as_str(),
            config.speakers.unwrap_or(0),
        );
    } else {
        let _ = writeln!(buf, "  Diarizer  : off");
    }
    let _ = writeln!(buf, "  Auto-rename: {}", config.auto_rename);
    if let Some(meta) = audio::meta::load(input) {
        let start = meta.trim_start_ms;
        let end = meta.trim_end_ms;
        if start > 0 || end.is_some() {
            let _ = writeln!(
                buf,
                "  Trim      : {}ms–{}",
                start,
                end.map_or_else(|| "end".into(), |e| format!("{e}ms")),
            );
        }
    }
    let _ = writeln!(
        buf,
        "System    : {} cores ({}), pid={}",
        std::thread::available_parallelism().map_or(0, std::num::NonZero::get),
        std::env::consts::ARCH,
        std::process::id(),
    );
    if let Ok(meta) = std::fs::metadata(input) {
        let _ = writeln!(buf, "Input     : {} bytes", meta.len());
    }
    for line in buf.lines() {
        logfile::info(line);
    }
}

fn validate_transcription_model(config: &Config) -> Result<()> {
    let Some(model) = models::by_id(&config.model) else {
        return Ok(());
    };
    if model.family != Family::Asr {
        return Err(Error::Config(format!(
            "{} is not a transcription model",
            config.model
        )));
    }
    Ok(())
}

#[tauri::command]
pub fn cancel_transcribe(input: PathBuf) -> bool {
    let key = input.to_string_lossy().into_owned();
    let token = TRANSCRIBE_CANCELS
        .lock()
        .ok()
        .and_then(|cancels| cancels.get(&key).cloned());
    token.is_some_and(|cancel| {
        cancel.store(true, Ordering::SeqCst);
        logfile::info(&format!("cancel_transcribe {}", input.display()));
        true
    })
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
pub fn reset_transcript_cache() -> Result<u64> {
    transcriber::cache::clear_all()
}

#[tauri::command]
pub fn reset_audio_cache() -> Result<u64> {
    audio::clear_cache()
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
        return Err(Error::Config(format!("not a file: {}", source.display())));
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
pub fn read_audio_bytes(path: PathBuf) -> Result<Vec<u8>> {
    Ok(std::fs::read(&path)?)
}

#[tauri::command]
pub fn save_recording(workdir: PathBuf, filename: String, bytes: Vec<u8>) -> Result<PathBuf> {
    std::fs::create_dir_all(&workdir)?;
    let safe = filename
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect::<String>();
    let mut dst = workdir.join(&safe);
    if dst.exists() {
        let stem = std::path::Path::new(&safe)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("recording");
        let ext = std::path::Path::new(&safe)
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
    std::fs::write(&dst, &bytes)?;
    logfile::info(&format!(
        "save_recording {} bytes -> {}",
        bytes.len(),
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
        return Err(Error::Config(
            "new name must not contain path separators".into(),
        ));
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
    if let Err(e) = transcriber::cache::rename_source(&source, &dst) {
        logfile::warn(&format!("cache index rename failed: {e}"));
    }
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
    let utterances = transcript.utterances.len();
    let t0 = std::time::Instant::now();
    logfile::info(&format!(
        "auto-rename: suggesting from {utterances} utterances"
    ));
    let result =
        tokio::task::spawn_blocking(move || namer::suggest(&transcript, chrono::Local::now()))
            .await
            .map_err(|e| crate::error::Error::Transcribe(format!("task: {e}")))?;
    match &result {
        Ok(s) => logfile::info(&format!(
            "auto-rename: suggested '{}_{}' in {:.2}s",
            s.topic,
            s.stamp,
            t0.elapsed().as_secs_f64(),
        )),
        Err(e) => logfile::warn(&format!("auto-rename failed: {e}")),
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_catalog_model_with_matching_engine() {
        let cfg = Config::default();

        assert!(validate_transcription_model(&cfg).is_ok());
    }
}
