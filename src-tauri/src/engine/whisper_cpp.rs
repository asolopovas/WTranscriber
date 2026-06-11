#![allow(
    clippy::significant_drop_tightening,
    clippy::too_many_lines,
    clippy::items_after_statements
)]

use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Stdio},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicI32, Ordering},
    },
};

use serde::Deserialize;
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

use crate::{
    config::{Config, Device},
    error::{Error, Result},
    models,
    process::quiet_command,
    transcriber::{Segment, Token},
};

struct CtxCell {
    model_path: String,
    use_gpu: bool,
    _ctx: WhisperContext,
    state: Mutex<WhisperState>,
}

static CTX: OnceLock<Mutex<Option<CtxCell>>> = OnceLock::new();
static LOG_HOOKS: OnceLock<()> = OnceLock::new();

fn ctx_slot() -> &'static Mutex<Option<CtxCell>> {
    CTX.get_or_init(|| Mutex::new(None))
}

fn install_log_hooks() {
    LOG_HOOKS.get_or_init(whisper_rs::install_logging_hooks);
}

fn resolve_model_path(model_id: &str) -> Result<PathBuf> {
    let entry = models::by_id(model_id).ok_or_else(|| {
        Error::Config(format!(
            "unknown whisper-cpp model id `{model_id}` (run `wt models list`)"
        ))
    })?;
    let files = models::paths_for(entry)?;
    let path = files
        .into_iter()
        .find(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("bin"))
        })
        .ok_or_else(|| {
            Error::Transcribe(format!(
                "whisper-cpp model `{model_id}` has no .bin file in catalog"
            ))
        })?;
    if !path.exists() {
        return Err(Error::Transcribe(format!(
            "whisper-cpp model file missing: {} (install via `wt models install {model_id}`)",
            path.display(),
        )));
    }
    Ok(path)
}

fn ensure_state(model_path: &std::path::Path, use_gpu: bool) -> Result<()> {
    install_log_hooks();
    let mut slot = ctx_slot()
        .lock()
        .map_err(|e| Error::Transcribe(format!("whisper-cpp ctx lock: {e}")))?;
    let model_str = model_path
        .to_str()
        .ok_or_else(|| Error::Config("whisper-cpp model path is not UTF-8".into()))?
        .to_owned();
    if slot
        .as_ref()
        .is_some_and(|c| c.model_path == model_str && c.use_gpu == use_gpu)
    {
        return Ok(());
    }
    let mut params = WhisperContextParameters::default();
    params.use_gpu(use_gpu);
    let ctx = WhisperContext::new_with_params(&model_str, params)
        .map_err(|e| Error::Transcribe(format!("whisper-cpp init {model_str}: {e}")))?;
    let state = ctx
        .create_state()
        .map_err(|e| Error::Transcribe(format!("whisper-cpp state: {e}")))?;
    *slot = Some(CtxCell {
        model_path: model_str,
        use_gpu,
        _ctx: ctx,
        state: Mutex::new(state),
    });
    Ok(())
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
const fn t_centisec_to_ms(t: i64) -> u64 {
    if t < 0 {
        return 0;
    }
    (t as u64).saturating_mul(10)
}

#[derive(Deserialize)]
struct WorkerOutput {
    segments: Vec<Segment>,
    language: String,
    rtf: f64,
}

pub fn cuda_worker_available() -> bool {
    cuda_worker_path().is_ok()
}

struct WorkerProc {
    child: Child,
    stdin: ChildStdin,
    reader: BufReader<ChildStdout>,
    model_path: String,
}

static WORKER: OnceLock<Mutex<Option<WorkerProc>>> = OnceLock::new();
static SERVE_UNSUPPORTED: AtomicBool = AtomicBool::new(false);

fn worker_proc_slot() -> &'static Mutex<Option<WorkerProc>> {
    WORKER.get_or_init(|| Mutex::new(None))
}

pub fn shutdown_worker() {
    if let Ok(mut slot) = worker_proc_slot().lock() {
        kill_worker(&mut slot);
    }
}

fn kill_worker(slot: &mut Option<WorkerProc>) {
    if let Some(mut w) = slot.take() {
        let _ = w.child.kill();
        let _ = w.child.wait();
    }
}

struct ServeFailure {
    handshake: bool,
    msg: String,
}

fn spawn_serve_worker(
    worker: &Path,
    model_path: &str,
) -> std::result::Result<WorkerProc, ServeFailure> {
    let fail = |handshake: bool, msg: String| ServeFailure { handshake, msg };
    let mut child = quiet_command(worker.as_os_str())
        .arg("--model")
        .arg(model_path)
        .arg("--serve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| fail(true, format!("spawn: {e}")))?;
    let Some(stdin) = child.stdin.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(fail(true, "worker has no stdin".into()));
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(fail(true, "worker has no stdout".into()));
    };
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(n) if n > 0 && line.contains("\"ready\"") => Ok(WorkerProc {
            child,
            stdin,
            reader,
            model_path: model_path.to_owned(),
        }),
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            Err(fail(true, "worker does not support serve mode".into()))
        }
    }
}

fn serve_request(
    audio_path: &Path,
    audio_dur_sec: f64,
    config: &Config,
    worker: &Path,
    model_path: &str,
) -> std::result::Result<(Vec<Segment>, String, f64), ServeFailure> {
    let fail = |msg: String| ServeFailure {
        handshake: false,
        msg,
    };
    let mut slot = worker_proc_slot()
        .lock()
        .map_err(|e| fail(format!("worker lock: {e}")))?;
    let reusable = slot.as_mut().is_some_and(|w| {
        w.model_path == model_path && w.child.try_wait().map(|s| s.is_none()).unwrap_or(false)
    });
    if !reusable {
        kill_worker(&mut slot);
        *slot = Some(spawn_serve_worker(worker, model_path)?);
    }
    let w = slot
        .as_mut()
        .ok_or_else(|| fail("worker slot empty".into()))?;
    let req = serde_json::json!({
        "audio_f32le": audio_path,
        "duration_sec": audio_dur_sec,
        "language": config.language,
        "threads": crate::engine::threads(config),
    });
    if let Err(e) = writeln!(w.stdin, "{req}").and_then(|()| w.stdin.flush()) {
        kill_worker(&mut slot);
        return Err(fail(format!("worker stdin: {e}")));
    }
    let mut line = String::new();
    match w.reader.read_line(&mut line) {
        Ok(n) if n > 0 => {}
        _ => {
            kill_worker(&mut slot);
            return Err(fail("worker closed pipe mid-job".into()));
        }
    }
    if let Ok(parsed) = serde_json::from_str::<WorkerOutput>(line.trim()) {
        return Ok((parsed.segments, parsed.language, parsed.rtf));
    }
    #[derive(Deserialize)]
    struct ServeErr {
        error: String,
    }
    let msg = serde_json::from_str::<ServeErr>(line.trim()).map_or_else(
        |_| format!("unparseable worker reply: {}", line.trim()),
        |e| e.error,
    );
    Err(fail(msg))
}

fn cuda_worker_path() -> Result<PathBuf> {
    if let Some(p) = std::env::var_os("WT_CUDA_WORKER") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Ok(p);
        }
    }
    let exe = std::env::current_exe()?;
    let install_dir = exe
        .parent()
        .ok_or_else(|| Error::Config("cannot resolve application directory".into()))?;
    let worker = install_dir
        .join("runtime")
        .join("cuda")
        .join("wt-whisper-cuda-worker.exe");
    if worker.exists() {
        return Ok(worker);
    }
    Err(Error::Config(format!(
        "Whisper CUDA worker is not installed at {}. Re-run the installer with NVIDIA GPU access, or set WT_CUDA_WORKER to a downloaded worker executable.",
        worker.display()
    )))
}

fn write_samples(samples: &[f32]) -> Result<tempfile::NamedTempFile> {
    let mut tmp = tempfile::Builder::new()
        .prefix("wtranscriber-whisper-")
        .suffix(".f32le")
        .tempfile()?;
    for sample in samples {
        tmp.write_all(&sample.to_le_bytes())?;
    }
    tmp.flush()?;
    Ok(tmp)
}

fn run_cuda_worker(
    samples: &[f32],
    audio_dur_sec: f64,
    config: &Config,
    model_path: &Path,
    on_progress: &mut dyn FnMut(f64),
    cancelled: &dyn Fn() -> bool,
) -> Result<(Vec<Segment>, String, f64)> {
    if cancelled() {
        return Err(Error::Cancelled);
    }
    let worker = cuda_worker_path()?;
    let audio = write_samples(samples)?;
    if !SERVE_UNSUPPORTED.load(Ordering::Relaxed) {
        let model_str = model_path
            .to_str()
            .ok_or_else(|| Error::Config("whisper-cpp model path is not UTF-8".into()))?;
        match serve_request(audio.path(), audio_dur_sec, config, &worker, model_str) {
            Ok(out) => {
                if cancelled() {
                    return Err(Error::Cancelled);
                }
                on_progress(100.0);
                return Ok(out);
            }
            Err(f) => {
                if f.handshake {
                    SERVE_UNSUPPORTED.store(true, Ordering::Relaxed);
                }
                crate::logfile::warn(&format!(
                    "CUDA worker serve mode unavailable ({}); falling back to one-shot worker",
                    f.msg
                ));
            }
        }
    }
    let output = quiet_command(worker.as_os_str())
        .arg("--model")
        .arg(model_path)
        .arg("--audio-f32le")
        .arg(audio.path())
        .arg("--duration-sec")
        .arg(format!("{audio_dur_sec:.6}"))
        .arg("--language")
        .arg(&config.language)
        .arg("--threads")
        .arg(crate::engine::threads(config).to_string())
        .output()
        .map_err(|e| Error::Transcribe(format!("launch CUDA worker {}: {e}", worker.display())))?;
    if cancelled() {
        return Err(Error::Cancelled);
    }
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Transcribe(format!(
            "CUDA worker failed ({}): {}",
            output.status,
            stderr.trim()
        )));
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|e| Error::Transcribe(format!("CUDA worker returned non-UTF8 JSON: {e}")))?;
    let parsed: WorkerOutput = serde_json::from_str(stdout.trim())?;
    on_progress(100.0);
    Ok((parsed.segments, parsed.language, parsed.rtf))
}

pub fn run(
    samples: &[f32],
    audio_dur_sec: f64,
    config: &Config,
    on_progress: &mut dyn FnMut(f64),
    cancelled: &dyn Fn() -> bool,
) -> Result<(Vec<Segment>, String, f64)> {
    let model_path = resolve_model_path(&config.model)?;
    let use_gpu = cfg!(feature = "cuda") && matches!(config.device, Device::Cuda);
    if matches!(config.device, Device::Cuda) && !use_gpu {
        return run_cuda_worker(
            samples,
            audio_dur_sec,
            config,
            &model_path,
            on_progress,
            cancelled,
        );
    }
    ensure_state(&model_path, use_gpu)?;
    let slot = ctx_slot()
        .lock()
        .map_err(|e| Error::Transcribe(format!("whisper-cpp slot lock: {e}")))?;
    let cell = slot
        .as_ref()
        .ok_or_else(|| Error::Transcribe("whisper-cpp ctx not initialised".into()))?;
    let mut state = cell
        .state
        .lock()
        .map_err(|e| Error::Transcribe(format!("whisper-cpp state lock: {e}")))?;

    let lang = config.language.trim();
    let lang_arg = (!lang.is_empty() && !lang.eq_ignore_ascii_case("auto")).then_some(lang);

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(lang_arg);
    if config.precise_word_timestamps {
        params.set_token_timestamps(true);
        params.set_split_on_word(true);
        params.set_max_len(1);
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    params.set_n_threads(i32::try_from(crate::engine::threads(config)).unwrap_or(4));
    params.set_no_context(true);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    params.set_print_timestamps(false);
    params.set_debug_mode(false);
    params.set_translate(false);
    params.set_single_segment(false);

    if config.debug_logging {
        let last_progress_step = Arc::new(AtomicI32::new(0));
        let progress_step = Arc::clone(&last_progress_step);
        params.set_progress_callback_safe(move |pct| {
            let step = (pct / 10) * 10;
            if step <= 0 || step >= 100 {
                return;
            }
            let previous = progress_step.load(Ordering::Relaxed);
            if step > previous
                && progress_step
                    .compare_exchange(previous, step, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
            {
                crate::logfile::debug(&format!("whisper-cpp progress {step}%"));
            }
        });
    }

    if cancelled() {
        return Err(Error::Cancelled);
    }
    let t0 = std::time::Instant::now();
    state
        .full(params, samples)
        .map_err(|e| Error::Transcribe(format!("whisper-cpp full: {e}")))?;
    on_progress(100.0);
    let elapsed = t0.elapsed().as_secs_f64();
    if cancelled() {
        return Err(Error::Cancelled);
    }

    let n = state.full_n_segments();
    let mut segs: Vec<Segment> = Vec::new();
    let mut current: Option<Segment> = None;
    fn flush(current: &mut Option<Segment>, segs: &mut Vec<Segment>) {
        if let Some(seg) = current.take() {
            if !seg.text.trim().is_empty() {
                segs.push(seg);
            }
        }
    }

    for i in 0..n {
        let Some(seg_handle) = state.get_segment(i) else {
            continue;
        };
        let raw = seg_handle
            .to_str_lossy()
            .map_err(|e| Error::Transcribe(format!("whisper-cpp seg text {i}: {e}")))?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let start_ms = t_centisec_to_ms(seg_handle.start_timestamp());
        let end_ms = t_centisec_to_ms(seg_handle.end_timestamp()).max(start_ms);
        let token = Token {
            text: trimmed.to_owned(),
            start_ms,
            end_ms,
            confidence: 0.0,
        };
        let starts_word = !trimmed.starts_with(|c: char| {
            !c.is_whitespace() && (c.is_ascii_punctuation() || c == '\u{2019}')
        });
        if starts_word || current.is_none() {
            flush(&mut current, &mut segs);
            current = Some(Segment {
                text: trimmed.to_owned(),
                start_ms,
                end_ms,
                tokens: vec![token],
            });
        } else if let Some(seg) = current.as_mut() {
            if !seg.text.ends_with(char::is_whitespace) && !trimmed.starts_with(' ') {
                seg.text.push(' ');
            }
            seg.text.push_str(trimmed);
            seg.end_ms = end_ms.max(seg.end_ms);
            seg.tokens.push(token);
        }
    }
    flush(&mut current, &mut segs);

    let detected_idx = state.full_lang_id_from_state();
    let detected = if detected_idx >= 0 {
        whisper_rs::get_lang_str(detected_idx).map_or_else(|| lang.to_owned(), str::to_owned)
    } else {
        lang.to_owned()
    };

    let rtf = if elapsed > 0.0 {
        audio_dur_sec / elapsed
    } else {
        0.0
    };
    Ok((segs, detected, rtf))
}
