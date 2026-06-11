#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::items_after_statements,
    clippy::too_many_lines
)]

use std::{
    fs,
    io::{BufRead, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    time::Instant,
};

use clap::Parser;
use serde::{Deserialize, Serialize};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

#[derive(Parser, Debug)]
#[command(name = "wt-whisper-cuda-worker", version)]
struct Cli {
    #[arg(long)]
    model: PathBuf,
    #[arg(long)]
    audio_f32le: Option<PathBuf>,
    #[arg(long, default_value_t = 0.0)]
    duration_sec: f64,
    #[arg(long, default_value = "auto")]
    language: String,
    #[arg(long, default_value_t = 4)]
    threads: u32,
    #[arg(long)]
    serve: bool,
}

#[derive(Debug, Deserialize)]
struct JobRequest {
    audio_f32le: PathBuf,
    #[serde(default)]
    duration_sec: f64,
    #[serde(default)]
    language: String,
    #[serde(default = "default_threads")]
    threads: u32,
}

const fn default_threads() -> u32 {
    4
}

#[derive(Debug, Serialize)]
struct ServeReady {
    ready: bool,
}

#[derive(Debug, Serialize)]
struct ServeError {
    error: String,
}

#[derive(Debug, Serialize)]
struct WorkerOutput {
    segments: Vec<Segment>,
    language: String,
    rtf: f64,
}

#[derive(Debug, Serialize)]
struct Segment {
    text: String,
    start_ms: u64,
    end_ms: u64,
    tokens: Vec<Token>,
}

#[derive(Debug, Serialize)]
struct Token {
    text: String,
    start_ms: u64,
    end_ms: u64,
    confidence: f32,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if cli.serve {
        return serve(&cli);
    }
    match run_oneshot(&cli) {
        Ok(out) => {
            println!(
                "{}",
                serde_json::to_string(&out).expect("serialise worker output")
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn serve(cli: &Cli) -> ExitCode {
    let engine = match Engine::new(&cli.model) {
        Ok(engine) => engine,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "{}",
        serde_json::to_string(&ServeReady { ready: true }).expect("serialise ready")
    );
    let _ = std::io::stdout().flush();
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<JobRequest>(&line)
            .map_err(|e| format!("bad request: {e}"))
            .and_then(|job| engine.transcribe(&job))
        {
            Ok(out) => serde_json::to_string(&out).expect("serialise worker output"),
            Err(error) => serde_json::to_string(&ServeError { error }).expect("serialise error"),
        };
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{reply}");
        let _ = out.flush();
    }
    ExitCode::SUCCESS
}

fn run_oneshot(cli: &Cli) -> Result<WorkerOutput, String> {
    let audio = cli
        .audio_f32le
        .clone()
        .ok_or_else(|| "--audio-f32le is required without --serve".to_string())?;
    let engine = Engine::new(&cli.model)?;
    engine.transcribe(&JobRequest {
        audio_f32le: audio,
        duration_sec: cli.duration_sec,
        language: cli.language.clone(),
        threads: cli.threads,
    })
}

struct Engine {
    ctx: WhisperContext,
}

impl Engine {
    fn new(model: &Path) -> Result<Self, String> {
        if !cfg!(feature = "cuda") {
            return Err("worker was built without CUDA support".into());
        }
        let model = model
            .to_str()
            .ok_or_else(|| "model path is not UTF-8".to_string())?;
        let mut ctx_params = WhisperContextParameters::default();
        ctx_params.use_gpu(true);
        let ctx = WhisperContext::new_with_params(model, ctx_params)
            .map_err(|e| format!("whisper-cpp init {model}: {e}"))?;
        Ok(Self { ctx })
    }

    fn transcribe(&self, job: &JobRequest) -> Result<WorkerOutput, String> {
        let samples = read_f32le(&job.audio_f32le)?;
        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| format!("whisper-cpp state: {e}"))?;

        let lang = job.language.trim();
        let lang_arg = (!lang.is_empty() && !lang.eq_ignore_ascii_case("auto")).then_some(lang);

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(lang_arg);
        params.set_token_timestamps(true);
        params.set_split_on_word(true);
        params.set_max_len(1);
        params.set_n_threads(i32::try_from(job.threads).unwrap_or(4));
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);
        params.set_translate(false);
        params.set_single_segment(false);
        params.set_no_context(true);

        let t0 = Instant::now();
        state
            .full(params, &samples)
            .map_err(|e| format!("whisper-cpp full: {e}"))?;
        let elapsed = t0.elapsed().as_secs_f64();
        let segments = collect_segments(&state)?;
        let detected_idx = state.full_lang_id_from_state();
        let language = if detected_idx >= 0 {
            whisper_rs::get_lang_str(detected_idx).map_or_else(|| lang.to_owned(), str::to_owned)
        } else {
            lang.to_owned()
        };
        let rtf = if elapsed > 0.0 {
            job.duration_sec / elapsed
        } else {
            0.0
        };
        Ok(WorkerOutput {
            segments,
            language,
            rtf,
        })
    }
}

fn read_f32le(path: &PathBuf) -> Result<Vec<f32>, String> {
    let bytes = fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    if bytes.len() % 4 != 0 {
        return Err(format!(
            "audio raw file length is not divisible by 4: {}",
            path.display()
        ));
    }
    Ok(bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect())
}

const fn t_centisec_to_ms(t: i64) -> u64 {
    if t < 0 {
        return 0;
    }
    (t as u64).saturating_mul(10)
}

fn collect_segments(state: &whisper_rs::WhisperState) -> Result<Vec<Segment>, String> {
    let n = state.full_n_segments();
    let mut segs: Vec<Segment> = Vec::new();
    let mut current: Option<Segment> = None;
    fn flush(current: &mut Option<Segment>, segs: &mut Vec<Segment>) {
        if let Some(seg) = current.take()
            && !seg.text.trim().is_empty()
        {
            segs.push(seg);
        }
    }

    for i in 0..n {
        let Some(seg_handle) = state.get_segment(i) else {
            continue;
        };
        let raw = seg_handle
            .to_str_lossy()
            .map_err(|e| format!("whisper-cpp seg text {i}: {e}"))?;
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
    Ok(segs)
}
