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

use super::config::sync_engine;
use crate::{
    audio,
    config::Config,
    error::{Error, Result},
    logfile,
    models::{self, Family},
    paths,
    progress::{self, DiarizeSmoother, Phase, Sink, Smoother},
    transcriber::{self, Job, Transcript},
};

static TRANSCRIBE_CANCELS: LazyLock<Mutex<HashMap<String, Arc<AtomicBool>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    path: String,
    phase: Phase,
    display_pct: f64,
    elapsed_sec: f64,
    eta_sec: f64,
}

#[derive(Clone, Copy)]
struct OverallInputs {
    phase: Phase,
    audio_dur_sec: f64,
    expect_diarize: bool,
    diarize_prior_rtf: f64,
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

    let total_wall = t_total + d_total;
    if total_wall <= 0.0 {
        return (0.0, 0.0);
    }
    let combined_pct = t_total.mul_add(t_pct, d_total * d_pct) / total_wall;
    let combined_eta = (t_remaining + d_remaining).max(0.0);
    (combined_pct.clamp(0.0, 99.5), combined_eta)
}

struct TranscribeSink {
    app: AppHandle,
    handle: Handle,
    file_path: String,
    audio_dur_sec: f64,
    expect_diarize: bool,
    diarize_prior_rtf: f64,
    diarize_backend: Mutex<String>,
    smoother: Arc<Mutex<Smoother>>,
    diarize: Arc<Mutex<Option<DiarizeSmoother>>>,
    current_phase: Arc<Mutex<Phase>>,
    ticker_cancel: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
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
        diarize_backend: String,
        diarize_prior_rtf: f64,
        cancel: Arc<AtomicBool>,
    ) -> Self {
        Self {
            app,
            handle,
            file_path,
            audio_dur_sec,
            expect_diarize,
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
        let elapsed_sec = self
            .smoother
            .lock()
            .map_or(0.0, |s| s.elapsed().as_secs_f64());
        let (display_pct, eta_sec) = match phase {
            Phase::Done => (100.0, 0.0),
            Phase::Transcribing | Phase::Diarizing => self.compute_overall(phase),
            _ => (display_pct, eta_sec),
        };
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
                    },
                    &smoother,
                    &diarize,
                );
                let elapsed_sec = smoother.lock().map_or(0.0, |s| s.elapsed().as_secs_f64());
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
        self.cancel.load(Ordering::SeqCst)
    }

    fn set_diarize_backend(&self, name: &str) {
        self.update_diarize_backend(name);
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
        expect_diarize,
        diarize_backend_hint,
        diarize_prior_rtf,
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_catalog_model_with_matching_engine() {
        let cfg = Config::default();

        assert!(validate_transcription_model(&cfg).is_ok());
    }
}
