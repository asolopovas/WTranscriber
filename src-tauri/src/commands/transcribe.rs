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

use tokio_util::sync::CancellationToken;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::{runtime::Handle, task::JoinHandle};

use super::config::sync_engine;
use crate::{
    audio,
    config::Config,
    diarizer,
    error::{Error, Result},
    logfile,
    models::{self, Family},
    paths,
    progress::{self, DiarizeSmoother, Phase, Sink, Smoother},
    transcriber::{self, Job, Transcript, cache, rediarize_words},
};

static TRANSCRIBE_CANCELS: LazyLock<Mutex<HashMap<String, CancellationToken>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static TRANSCRIBE_LOCK: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    path: String,
    phase: Phase,
    display_pct: f64,
    elapsed_sec: f64,
    total_sec: f64,
}

#[derive(Clone, Copy)]
struct OverallInputs {
    phase: Phase,
    audio_dur_sec: f64,
    expect_diarize: bool,
    diarize_prior_rtf: f64,
    diarize_only: bool,
}

fn compute_overall_static(
    inputs: OverallInputs,
    smoother: &Mutex<Smoother>,
    diarize: &Mutex<Option<DiarizeSmoother>>,
) -> (f64, f64) {
    let rank = match inputs.phase {
        Phase::CacheCheck => 0,
        Phase::LoadingAudio => 1,
        Phase::Transcribing => 2,
        Phase::Diarizing => 3,
        Phase::Writing => 4,
        Phase::Done => 5,
    };
    let transcribe_done = rank > 2;

    let (t_total, t_pct, t_remaining) = smoother.lock().map_or_else(
        |_| {
            let dur = inputs.audio_dur_sec.max(1.0);
            (dur, 0.0, dur)
        },
        |mut s| {
            let total = s.total_wall_sec().max(0.001);
            if transcribe_done {
                (total, 100.0, 0.0)
            } else if matches!(inputs.phase, Phase::Transcribing) {
                let (pct, _) = s.snapshot();
                (total, pct, s.remaining_wall_sec())
            } else {
                (total, 0.0, total)
            }
        },
    );

    let (d_total, d_pct, d_remaining) = if inputs.expect_diarize {
        let diarize_done = rank > 3;
        let prior_total = (inputs.audio_dur_sec / inputs.diarize_prior_rtf).max(0.001);
        if diarize_done {
            (prior_total, 100.0, 0.0)
        } else {
            diarize
                .lock()
                .map_or((prior_total, 0.0, prior_total), |mut g| {
                    g.as_mut().map_or((prior_total, 0.0, prior_total), |d| {
                        let total = d.total_wall_sec().max(0.001);
                        let (pct, _) = d.snapshot();
                        (total, pct, d.remaining_wall_sec())
                    })
                })
        }
    } else {
        (0.0, 0.0, 0.0)
    };

    if inputs.diarize_only {
        return (d_pct.clamp(0.0, 99.5), d_remaining.max(0.0));
    }
    let total_wall = t_total + d_total;
    if total_wall <= 0.0 {
        return (0.0, 0.0);
    }
    let combined_pct = t_total.mul_add(t_pct, d_total * d_pct) / total_wall;
    let combined_eta = (t_remaining + d_remaining).max(0.0);
    (combined_pct.clamp(0.0, 99.5), combined_eta)
}

fn compute_total_sec(phase: Phase, display_pct: f64, elapsed_sec: f64, eta_sec: f64) -> f64 {
    if matches!(phase, Phase::Done) {
        return elapsed_sec;
    }
    let implied = if display_pct >= 1.0 {
        elapsed_sec / (display_pct / 100.0)
    } else {
        elapsed_sec + eta_sec
    };
    implied.max(elapsed_sec)
}

struct TranscribeSink {
    app: AppHandle,
    handle: Handle,
    file_path: String,
    audio_dur_sec: f64,
    expect_diarize: bool,
    diarize_only: bool,
    diarize_prior_rtf: f64,
    diarize_backend: Mutex<String>,
    smoother: Arc<Mutex<Smoother>>,
    diarize: Arc<Mutex<Option<DiarizeSmoother>>>,
    current_phase: Arc<Mutex<Phase>>,
    ticker_cancel: Arc<AtomicBool>,
    cancel: CancellationToken,
    ticker_handle: Mutex<Option<JoinHandle<()>>>,
}

impl TranscribeSink {
    #[allow(clippy::too_many_arguments)]
    fn new(
        app: AppHandle,
        handle: Handle,
        file_path: String,
        audio_dur_sec: f64,
        initial_rtf: f64,
        expect_diarize: bool,
        diarize_only: bool,
        diarize_backend: String,
        diarize_prior_rtf: f64,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            app,
            handle,
            file_path,
            audio_dur_sec,
            expect_diarize,
            diarize_only,
            diarize_prior_rtf,
            diarize_backend: Mutex::new(diarize_backend),
            smoother: Arc::new(Mutex::new(Smoother::new(audio_dur_sec, initial_rtf))),
            diarize: Arc::new(Mutex::new(None)),
            current_phase: Arc::new(Mutex::new(Phase::CacheCheck)),
            ticker_cancel: Arc::new(AtomicBool::new(false)),
            cancel,
            ticker_handle: Mutex::new(None),
        }
    }

    fn compute_overall(&self, phase: Phase) -> (f64, f64) {
        compute_overall_static(
            OverallInputs {
                phase,
                audio_dur_sec: self.audio_dur_sec,
                expect_diarize: self.expect_diarize,
                diarize_prior_rtf: self.diarize_prior_rtf,
                diarize_only: self.diarize_only,
            },
            &self.smoother,
            &self.diarize,
        )
    }

    fn save_diarize_rtf(&self) {
        let backend = self
            .diarize_backend
            .lock()
            .ok()
            .map(|g| g.clone())
            .unwrap_or_default();
        if backend.is_empty() {
            return;
        }
        let observed = self
            .diarize
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(DiarizeSmoother::observed_rtf))
            .unwrap_or(0.0);
        if observed > 0.0 {
            progress::save_diarize_rtf(&backend, observed);
        }
    }

    fn update_diarize_backend(&self, name: &str) {
        if let Ok(mut g) = self.diarize_backend.lock() {
            *g = name.to_string();
        }
    }

    fn emit(&self, phase: Phase, display_pct: f64, eta_sec: f64) {
        if phase != Phase::Done && self.cancel.is_cancelled() {
            return;
        }
        let elapsed_sec = self
            .smoother
            .lock()
            .map_or(0.0, |s| s.elapsed().as_secs_f64());
        let (display_pct, eta_sec) = match phase {
            Phase::Done => (100.0, 0.0),
            Phase::Transcribing | Phase::Diarizing => self.compute_overall(phase),
            _ => (display_pct, eta_sec),
        };
        let total_sec = compute_total_sec(phase, display_pct, elapsed_sec, eta_sec);
        let _ = self.app.emit(
            "transcribe:progress",
            &ProgressEvent {
                path: self.file_path.clone(),
                phase,
                display_pct,
                elapsed_sec,
                total_sec,
            },
        );
    }

    fn start_ticker(&self) {
        let mut handle_lock = self
            .ticker_handle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
        let audio_dur_sec = self.audio_dur_sec;
        let expect_diarize = self.expect_diarize;
        let diarize_only = self.diarize_only;
        let diarize_prior_rtf = self.diarize_prior_rtf;
        let join = self.handle.spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(500));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                if cancel.load(Ordering::SeqCst) {
                    break;
                }
                let phase = phase_lock.lock().ok().map_or(Phase::Transcribing, |g| *g);
                if !matches!(phase, Phase::Transcribing | Phase::Diarizing) {
                    continue;
                }
                let (display_pct, eta_sec) = compute_overall_static(
                    OverallInputs {
                        phase,
                        audio_dur_sec,
                        expect_diarize,
                        diarize_prior_rtf,
                        diarize_only,
                    },
                    &smoother,
                    &diarize,
                );
                let elapsed_sec = smoother.lock().map_or(0.0, |s| s.elapsed().as_secs_f64());
                let total_sec = compute_total_sec(phase, display_pct, elapsed_sec, eta_sec);
                let _ = app.emit(
                    "transcribe:progress",
                    &ProgressEvent {
                        path: path.clone(),
                        phase,
                        display_pct,
                        elapsed_sec,
                        total_sec,
                    },
                );
            }
        });
        *handle_lock = Some(join);
    }

    fn stop_ticker(&self) {
        self.ticker_cancel.store(true, Ordering::SeqCst);
        let taken = self
            .ticker_handle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
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
            *g = Some(DiarizeSmoother::new(
                self.audio_dur_sec,
                self.diarize_prior_rtf,
            ));
        }
        if matches!(phase, Phase::Done) {
            self.save_diarize_rtf();
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
                        *g = Some(DiarizeSmoother::new(
                            self.audio_dur_sec,
                            self.diarize_prior_rtf,
                        ));
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
        self.cancel.is_cancelled()
    }

    fn set_diarize_backend(&self, name: &str) {
        self.update_diarize_backend(name);
    }
}

struct TranscribeRunContext {
    label: String,
    display_name: String,
    input_key: String,
    sink: Arc<TranscribeSink>,
}

#[tauri::command]
pub async fn transcribe_file(
    app: AppHandle,
    input: PathBuf,
    mut config: Config,
) -> Result<Transcript> {
    sync_engine(&mut config);
    validate_transcription_model(&config)?;
    let input_key = input.to_string_lossy().into_owned();
    let cancel = CancellationToken::new();
    register_cancel(&input_key, cancel.clone());
    let (lock_contended, _guard) = wait_for_transcribe_slot(&input).await;
    if cancel.is_cancelled() {
        logfile::info(&format!("cancelled before start ({})", input.display()));
        unregister_cancel(&input_key);
        return Err(Error::Cancelled);
    }
    if lock_contended {
        logfile::info(&format!(
            "queue resumed: starting transcribe ({})",
            input.display()
        ));
    }
    let context = prepare_transcribe_run(app, &input, &config, input_key.clone(), cancel.clone());
    let mut run_handle =
        spawn_transcribe_job(input, config, context.sink.clone(), context.label.clone());
    let raced = race_transcribe_job(&mut run_handle, cancel).await;
    let result = finish_transcribe_run(raced, &context);
    crate::android_stop_transcription_service();
    unregister_cancel(&context.input_key);
    result
}

async fn wait_for_transcribe_slot(input: &Path) -> (bool, tokio::sync::MutexGuard<'static, ()>) {
    let lock_contended = TRANSCRIBE_LOCK.try_lock().is_err();
    if lock_contended {
        logfile::info(&format!(
            "queued: waiting for previous transcription to finish ({})",
            input.display()
        ));
    }
    let guard = TRANSCRIBE_LOCK.lock().await;
    (lock_contended, guard)
}

fn prepare_transcribe_run(
    app: AppHandle,
    input: &Path,
    config: &Config,
    input_key: String,
    cancel: CancellationToken,
) -> TranscribeRunContext {
    let label = format!("transcribe {}", input.display());
    logfile::process_start(&label);
    log_preflight(input, config);
    let display_name = input
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("audio")
        .to_string();
    crate::android_start_transcription_service(&format!("Transcribing {display_name}"));
    let audio_dur_sec =
        audio::probe_duration_ms(input).map_or(1.0, |ms| (ms as f64 / 1000.0).max(1.0));
    let device_label = config.device.as_str().to_owned();
    let expect_diarize = config.diarize;
    let diarize_backend_hint = if expect_diarize {
        config.diarizer.as_str().to_string()
    } else {
        String::new()
    };
    let diarize_prior_rtf = if expect_diarize {
        progress::load_diarize_rtf(&diarize_backend_hint)
    } else {
        progress::DIARIZE_DEFAULT_RTF
    };
    let sink = Arc::new(TranscribeSink::new(
        app,
        Handle::current(),
        input_key.clone(),
        audio_dur_sec,
        progress::load_rtf(&config.model, &device_label),
        expect_diarize,
        false,
        diarize_backend_hint,
        diarize_prior_rtf,
        cancel,
    ));
    TranscribeRunContext {
        label,
        display_name,
        input_key,
        sink,
    }
}

fn spawn_transcribe_job(
    input: PathBuf,
    config: Config,
    sink: Arc<TranscribeSink>,
    label: String,
) -> JoinHandle<Result<Transcript>> {
    tokio::spawn(async move {
        let job = Job { input, config };
        let res = transcriber::run_with_sink(&job, sink).await;
        match &res {
            Ok(_) => logfile::info(&format!("engine returned naturally ({label})")),
            Err(Error::Cancelled) => {
                logfile::info(&format!("engine finalised after cancel ({label})"));
            }
            Err(e) => logfile::info(&format!("engine finalised with error ({label}): {e}")),
        }
        res
    })
}

async fn race_transcribe_job(
    run_handle: &mut JoinHandle<Result<Transcript>>,
    cancel: CancellationToken,
) -> Result<Transcript> {
    tokio::select! {
        biased;
        joined = run_handle => match joined {
            Ok(inner) => inner,
            Err(e) => Err(Error::Other(anyhow::anyhow!("join: {e}"))),
        },
        () = cancel.cancelled() => {
            logfile::info("cancel acknowledged; releasing lock and detaching engine task");
            Err(Error::Cancelled)
        },
    }
}

fn finish_transcribe_run(
    raced: Result<Transcript>,
    context: &TranscribeRunContext,
) -> Result<Transcript> {
    match raced {
        Ok(t) => Ok(finish_successful_transcribe(t, context)),
        Err(Error::Cancelled) => {
            logfile::process_end(&context.label, "cancelled", "user cancelled");
            Err(Error::Cancelled)
        }
        Err(e) => {
            logfile::error(&format!("{}: {e}", context.label));
            logfile::process_end(&context.label, "failed", &e.to_string());
            crate::android_notify_transcription_done(
                "Transcription failed",
                &format!("{}: {e}", context.display_name),
                false,
            );
            Err(e)
        }
    }
}

fn finish_successful_transcribe(
    transcript: Transcript,
    context: &TranscribeRunContext,
) -> Transcript {
    logfile::process_end(
        &context.label,
        "ok",
        &format!(
            "{} utterances, {} ms, {} speakers",
            transcript.utterances.len(),
            transcript.duration_ms,
            transcript.speakers_detected
        ),
    );
    context.sink.emit(Phase::Done, 100.0, 0.0);
    crate::android_notify_transcription_done(
        "Transcription finished",
        &format!(
            "{}: {} utterances",
            context.display_name,
            transcript.utterances.len()
        ),
        true,
    );
    transcript
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
pub async fn redo_diarization(
    app: AppHandle,
    input: PathBuf,
    old_cache_key: String,
    mut config: Config,
) -> Result<Transcript> {
    sync_engine(&mut config);
    let label = format!("redo_diarization {}", input.display());
    logfile::process_start(&label);
    let display_name = input
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("audio")
        .to_string();
    crate::android_start_transcription_service(&format!("Re-diarizing {display_name}"));

    let audio_dur_ms = audio::probe_duration_ms(&input).unwrap_or(0);
    let audio_dur_sec = if audio_dur_ms > 0 {
        audio_dur_ms as f64 / 1000.0
    } else {
        1.0
    };
    let device_label = config.device.as_str().to_owned();
    let initial_rtf = progress::load_rtf(&config.model, &device_label);
    let diarize_backend_hint = config.diarizer.as_str().to_string();
    let diarize_prior_rtf = progress::load_diarize_rtf(&diarize_backend_hint);
    let input_key = input.to_string_lossy().into_owned();
    let cancel = CancellationToken::new();
    register_cancel(&input_key, cancel.clone());
    let sink = Arc::new(TranscribeSink::new(
        app,
        Handle::current(),
        input_key.clone(),
        audio_dur_sec,
        initial_rtf,
        true,
        true,
        diarize_backend_hint,
        diarize_prior_rtf,
        cancel.clone(),
    ));

    let cancel_watch = cancel.clone();
    let inner_fut = redo_diarization_inner(input, old_cache_key, config, sink.clone());
    let result = tokio::select! {
        biased;
        res = inner_fut => res,
        () = cancel_watch.cancelled() => Err(Error::Cancelled),
    };
    let outcome = match result {
        Ok(t) => {
            logfile::process_end(
                &label,
                "ok",
                &format!(
                    "{} utterances, {} speakers",
                    t.utterances.len(),
                    t.speakers_detected
                ),
            );
            sink.emit(Phase::Done, 100.0, 0.0);
            crate::android_notify_transcription_done(
                "Re-diarization finished",
                &format!("{display_name}: {} speakers", t.speakers_detected),
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
                "Re-diarization failed",
                &format!("{display_name}: {e}"),
                false,
            );
            Err(e)
        }
    };
    crate::android_stop_transcription_service();
    unregister_cancel(&input_key);
    outcome
}

async fn redo_diarization_inner(
    input: PathBuf,
    old_cache_key: String,
    config: Config,
    sink: Arc<TranscribeSink>,
) -> Result<Transcript> {
    tokio::task::spawn_blocking(move || -> Result<Transcript> {
        let cached = cache::load(&old_cache_key)?
            .ok_or_else(|| Error::Config("no cached transcript to re-diarize".into()))?;
        if sink.is_cancelled() {
            return Err(Error::Cancelled);
        }
        sink.phase(Phase::Diarizing);
        let speakers = config.speakers.unwrap_or(0);
        let wav = audio::ensure_cached_wav(&input)?;
        let backend = diarizer::new_with_choice(speakers, config.diarizer)?;
        let backend_name = backend.name();
        sink.set_diarize_backend(&backend_name);
        let mut on_progress = |pct: f64| sink.report_pct(Phase::Diarizing, pct);
        let cancelled = || sink.is_cancelled();
        let diar_t0 = std::time::Instant::now();
        let segs = backend
            .diarize(
                &wav,
                speakers,
                cached.duration_ms as f64 / 1000.0,
                &cancelled,
                &mut on_progress,
            )
            .map_err(|e| Error::Transcribe(format!("diarize: {e}")))?;
        logfile::info(&format!(
            "re-diarized: {backend_name} · {} segments · {:.1}s",
            segs.len(),
            diar_t0.elapsed().as_secs_f64(),
        ));

        sink.phase(Phase::Writing);
        let trim = audio::meta::load(&input).unwrap_or_default();
        let key_params = cache::build_key_params(
            &input,
            &cached.model,
            &cached.language,
            speakers,
            !config.diarize,
            trim.trim_start_ms,
            trim.trim_end_ms.unwrap_or(0),
        )?;
        let new_key = cache::compute_key(&key_params);

        let new_transcript = rediarize_words(
            cached.words.clone(),
            &segs,
            transcriber::Meta {
                model: cached.model.clone(),
                language: cached.language.clone(),
                duration_ms: cached.duration_ms,
                diarizer: Some(backend_name),
                device: cached.device.clone(),
            },
        );

        let entry = cache::Entry {
            key: new_key.clone(),
            source_path: key_params.source_path.clone(),
            source_name: key_params
                .source_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            model: cached.model.clone(),
            language: cached.language.clone(),
            speakers,
            no_diarize: !config.diarize,
            utterances: new_transcript.utterances.len(),
            duration_ms: cached.duration_ms,
            created_at: chrono::Utc::now(),
            size_bytes: 0,
        };
        cache::store(entry, &new_transcript)?;
        if new_key != old_cache_key {
            let _ = cache::invalidate(&old_cache_key);
        }
        Ok(new_transcript)
    })
    .await
    .map_err(|e| Error::Other(anyhow::anyhow!("join: {e}")))?
}

#[tauri::command]
pub fn cancel_all_transcribes() -> usize {
    cancel_all()
}

pub(super) fn register_cancel(key: &str, token: CancellationToken) {
    if let Ok(mut cancels) = TRANSCRIBE_CANCELS.lock() {
        cancels.insert(key.to_string(), token);
    }
}

pub(super) fn unregister_cancel(key: &str) {
    if let Ok(mut cancels) = TRANSCRIBE_CANCELS.lock() {
        cancels.remove(key);
    }
}

pub(super) fn cancel_all() -> usize {
    let Ok(cancels) = TRANSCRIBE_CANCELS.lock() else {
        return 0;
    };
    let mut count = 0_usize;
    for (key, token) in cancels.iter() {
        token.cancel();
        logfile::info(&format!("cancel_transcribe {key}"));
        count += 1;
    }
    count
}

#[cfg(test)]
pub(super) fn registered_cancels_len() -> usize {
    TRANSCRIBE_CANCELS.lock().map(|c| c.len()).unwrap_or(0)
}

#[cfg(test)]
pub(super) fn lock() -> &'static tokio::sync::Mutex<()> {
    &TRANSCRIBE_LOCK
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex as StdMutex, OnceLock};
    use std::time::Instant;

    fn cancel_test_lock() -> &'static StdMutex<()> {
        static LOCK: OnceLock<StdMutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| StdMutex::new(()))
    }

    fn clear_cancels() {
        if let Ok(mut g) = TRANSCRIBE_CANCELS.lock() {
            g.clear();
        }
    }

    #[test]
    fn accepts_catalog_model_with_matching_engine() {
        let cfg = Config::default();

        assert!(validate_transcription_model(&cfg).is_ok());
    }

    #[test]
    fn cancel_all_cancels_every_token() {
        let _g = cancel_test_lock().lock().unwrap();
        clear_cancels();
        let a = CancellationToken::new();
        let b = CancellationToken::new();
        let c = CancellationToken::new();
        register_cancel("/tmp/a", a.clone());
        register_cancel("/tmp/b", b.clone());
        register_cancel("/tmp/c", c.clone());
        assert_eq!(registered_cancels_len(), 3);
        let n = cancel_all();
        assert_eq!(n, 3);
        assert!(a.is_cancelled());
        assert!(b.is_cancelled());
        assert!(c.is_cancelled());
        clear_cancels();
    }

    #[test]
    fn unregister_removes_from_map() {
        let _g = cancel_test_lock().lock().unwrap();
        clear_cancels();
        register_cancel("/tmp/x", CancellationToken::new());
        assert_eq!(registered_cancels_len(), 1);
        unregister_cancel("/tmp/x");
        assert_eq!(registered_cancels_len(), 0);
    }

    #[tokio::test]
    async fn select_race_returns_cancelled_immediately() {
        let token = CancellationToken::new();
        let watch = token.clone();
        let long_running = async {
            tokio::time::sleep(Duration::from_secs(60)).await;
            42_u32
        };
        tokio::pin!(long_running);
        let cancel_at = Instant::now();
        tokio::spawn({
            let t = token.clone();
            async move {
                tokio::time::sleep(Duration::from_millis(10)).await;
                t.cancel();
            }
        });
        let cancelled = tokio::select! {
            biased;
            v = &mut long_running => Err::<(), u32>(v),
            () = watch.cancelled() => Ok(()),
        };
        assert!(cancelled.is_ok(), "cancel branch must win");
        assert!(
            cancel_at.elapsed() < Duration::from_millis(200),
            "cancel should return within 200ms, took {:?}",
            cancel_at.elapsed()
        );
    }

    #[test]
    fn queued_job_observes_cancel_before_lock_release() {
        let _g = cancel_test_lock().lock().unwrap();
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                clear_cancels();
                let lock = lock();
                let guard_a = lock.lock().await;

                let token_b = CancellationToken::new();
                register_cancel("/tmp/job-b", token_b.clone());
                let lock_for_b = lock;
                let token_b_inside = token_b.clone();
                let b = tokio::spawn(async move {
                    let _g = lock_for_b.lock().await;
                    token_b_inside.is_cancelled()
                });

                tokio::time::sleep(Duration::from_millis(20)).await;
                assert_eq!(cancel_all(), 1);
                drop(guard_a);

                let observed_cancel = b.await.unwrap();
                assert!(
                    observed_cancel,
                    "queued task must observe is_cancelled after lock acquisition"
                );
                unregister_cancel("/tmp/job-b");
            });
    }

    #[test]
    fn cancel_all_wakes_every_registered_select_race() {
        let _g = cancel_test_lock().lock().unwrap();
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                clear_cancels();
                let t1 = CancellationToken::new();
                let t2 = CancellationToken::new();
                register_cancel("/tmp/r1", t1.clone());
                register_cancel("/tmp/r2", t2.clone());
                let watch1 = t1.clone();
                let watch2 = t2.clone();
                let h1 = tokio::spawn(async move {
                    let work = async { tokio::time::sleep(Duration::from_secs(60)).await };
                    tokio::pin!(work);
                    tokio::select! {
                        () = &mut work => false,
                        () = watch1.cancelled() => true,
                    }
                });
                let h2 = tokio::spawn(async move {
                    let work = async { tokio::time::sleep(Duration::from_secs(60)).await };
                    tokio::pin!(work);
                    tokio::select! {
                        () = &mut work => false,
                        () = watch2.cancelled() => true,
                    }
                });
                tokio::time::sleep(Duration::from_millis(10)).await;
                let started = Instant::now();
                let n = cancel_all();
                assert_eq!(n, 2);
                let r1 = tokio::time::timeout(Duration::from_secs(1), h1)
                    .await
                    .unwrap()
                    .unwrap();
                let r2 = tokio::time::timeout(Duration::from_secs(1), h2)
                    .await
                    .unwrap()
                    .unwrap();
                assert!(r1, "task 1 must have been cancelled");
                assert!(r2, "task 2 must have been cancelled");
                assert!(
                    started.elapsed() < Duration::from_millis(500),
                    "cancel propagation took too long: {:?}",
                    started.elapsed()
                );
                clear_cancels();
            });
    }

    #[tokio::test]
    async fn sink_emit_drops_progress_after_cancel() {
        let token = CancellationToken::new();
        let dropped = phase_should_drop(Phase::Transcribing, &token);
        assert!(!dropped, "before cancel, transcribing events must pass");
        let dropped_done = phase_should_drop(Phase::Done, &token);
        assert!(!dropped_done, "Done events must pass before cancel");
        token.cancel();
        assert!(
            phase_should_drop(Phase::Transcribing, &token),
            "after cancel, non-Done events must be dropped"
        );
        assert!(
            !phase_should_drop(Phase::Done, &token),
            "after cancel, Done events must still pass"
        );
    }

    fn phase_should_drop(phase: Phase, token: &CancellationToken) -> bool {
        phase != Phase::Done && token.is_cancelled()
    }
}
