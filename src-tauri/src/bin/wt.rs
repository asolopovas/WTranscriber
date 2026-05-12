#![allow(clippy::needless_pass_by_value)]

use std::{
    io::Write,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::{Arc, Mutex},
};

use clap::{Parser, Subcommand};
use wtranscriber_lib::{
    api::{self, Config, Device, Engine, FileProgress, Job, Phase, Result, Sink, Transcript},
    logfile, namer,
};

#[derive(Parser, Debug)]
#[command(
    name = "wt",
    version,
    about = "WTranscriber CLI \u{2014} offline audio transcription + diarization",
    long_about = "WTranscriber CLI \u{2014} offline audio transcription + diarization.\n\n\
Accepts one or more audio files and writes a JSON transcript next to each input.\n\
Models are downloaded on demand into ~/.local/share/wtranscriber/models.\n\
A rolling log is written to ~/.local/share/wtranscriber/wt.log (same as the GUI).",
    after_help = "Examples:\n  \
      wt audio.wav                       # transcribe with defaults\n  \
      wt --device cpu --no-diarize a.wav # CPU, no diarization\n  \
      wt -l en --speakers 2 *.wav        # English, 2-speaker diarization\n  \
      wt --rename audio.wav              # LLM-suggested rename of the source file\n  \
      wt models list                     # show catalog + install state\n  \
      wt models install sherpa-whisper-turbo",
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(
        help = "One or more audio files to transcribe (wav/mp3/flac/m4a/ogg \u{2014} decoded via ffmpeg)"
    )]
    inputs: Vec<PathBuf>,

    #[arg(
        short,
        long,
        value_name = "LANG",
        help = "BCP-47 language code (e.g. `en`, `ru`, `auto`). Defaults to the saved config"
    )]
    lang: Option<String>,

    #[arg(
        short,
        long,
        value_name = "MODEL",
        help = "ASR model id from `wt models list` (e.g. `sherpa-whisper-turbo`)"
    )]
    model: Option<String>,

    #[arg(
        short,
        long,
        value_name = "N",
        help = "CPU thread count for the transcription engine (default: auto)"
    )]
    threads: Option<u32>,

    #[arg(
        long,
        value_name = "N",
        help = "Expected number of speakers (enables diarization with a fixed count)"
    )]
    speakers: Option<u32>,

    #[arg(long, help = "Disable speaker diarization for this run")]
    no_diarize: bool,

    #[arg(
        long,
        value_enum,
        value_name = "DEVICE",
        help = "Compute device: cpu or cuda (cuda requires a CUDA-enabled build)"
    )]
    device: Option<Device>,

    #[arg(
        long,
        value_enum,
        value_name = "ENGINE",
        help = "Override the ASR engine kind (advanced; defaults to the model's native engine)"
    )]
    engine: Option<Engine>,

    #[arg(
        long,
        help = "Ignore any cached transcript for this input and rerun from scratch"
    )]
    no_cache: bool,

    #[arg(
        long,
        help = "After transcribing, ask the local LLM for a sensible filename and rename the source"
    )]
    rename: bool,
}

#[derive(Subcommand, Debug)]
enum Command {
    #[command(about = "Manage local model catalog (list, install, status)")]
    Models {
        #[command(subcommand)]
        action: ModelsAction,
    },
}

#[derive(Subcommand, Debug)]
enum ModelsAction {
    #[command(about = "List all known models with size and install status")]
    List,
    #[command(about = "Download and install a model by id (see `wt models list`)")]
    Install {
        #[arg(help = "Model id, e.g. `sherpa-whisper-turbo`")]
        id: String,
    },
    #[command(about = "Print install status (installed | not_installed | partial) for a model id")]
    Status {
        #[arg(help = "Model id, e.g. `sherpa-whisper-turbo`")]
        id: String,
    },
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "debug,wtranscriber=trace,wtranscriber_lib=trace".into()),
        )
        .init();

    let cli = Cli::parse();

    logfile::info(&format!(
        "wt CLI v{} starting (cuda_feature={})",
        env!("CARGO_PKG_VERSION"),
        cfg!(feature = "cuda"),
    ));

    if cfg!(feature = "cuda") {
        wtranscriber_lib::cuda_setup::setup();
    }

    let result = if let Some(cmd) = cli.command {
        run_command(cmd).await
    } else {
        run_transcribe(cli).await
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

async fn run_command(cmd: Command) -> Result<()> {
    match cmd {
        Command::Models { action } => run_models(action).await,
    }
}

async fn run_models(action: ModelsAction) -> Result<()> {
    let mgr = api::manager();
    match action {
        ModelsAction::List => {
            let mut rows = mgr.list()?;
            rows.sort_by(|a, b| a.id.cmp(&b.id));
            for m in rows {
                println!(
                    "{:<32} {:<10} {:<14} {:>8} MB  {}",
                    m.id,
                    m.family.as_str(),
                    m.status.as_str(),
                    m.size_bytes / 1_048_576,
                    m.display_name,
                );
            }
        }
        ModelsAction::Install { id } => {
            let mut on_progress = print_progress;
            mgr.install(&id, &mut on_progress).await?;
            println!("\ninstalled: {id}");
        }
        ModelsAction::Status { id } => {
            let s = mgr.status(&id)?;
            println!("{}", s.as_str());
        }
    }
    Ok(())
}

#[allow(clippy::cast_precision_loss)]
fn print_progress(p: FileProgress) {
    let pct = if p.total == 0 {
        0.0
    } else {
        (p.downloaded as f64 / p.total as f64) * 100.0
    };
    eprint!(
        "\r[{}/{}] {:<48} {:>5.1}%",
        p.file_index + 1,
        p.file_count,
        truncate(&p.rel_path, 48),
        pct
    );
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        format!("{s:<n$}")
    } else {
        format!("…{}", &s[s.len() - (n - 1)..])
    }
}

struct CliSink {
    label: String,
    state: Mutex<CliSinkState>,
}

struct CliSinkState {
    last_phase: Option<Phase>,
    last_shown_pct: i32,
    start: std::time::Instant,
}

impl CliSink {
    fn new(input: PathBuf) -> Self {
        let label = input.file_name().map_or_else(
            || input.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        Self {
            label,
            state: Mutex::new(CliSinkState {
                last_phase: None,
                last_shown_pct: -1,
                start: std::time::Instant::now(),
            }),
        }
    }

    const fn phase_label(phase: Phase) -> &'static str {
        match phase {
            Phase::CacheCheck => "cache",
            Phase::LoadingAudio => "load",
            Phase::Transcribing => "asr",
            Phase::Diarizing => "diar",
            Phase::Writing => "write",
            Phase::Done => "done",
        }
    }

    fn render(&self, phase: Phase, pct: f64) {
        let Ok(mut s) = self.state.lock() else {
            return;
        };
        let new_phase = s.last_phase != Some(phase);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let pct_int = pct.clamp(0.0, 100.0).round() as i32;
        if !new_phase && pct_int == s.last_shown_pct && !matches!(phase, Phase::Done) {
            return;
        }
        if new_phase && s.last_phase.is_some() {
            eprintln!();
        }
        let elapsed = s.start.elapsed().as_secs_f64();
        let mut err = std::io::stderr().lock();
        let _ = write!(
            err,
            "\r[{phase}] {label:<32} {pct:>5.1}%  {elapsed:>5.1}s",
            phase = Self::phase_label(phase),
            label = truncate(&self.label, 32),
            pct = pct.clamp(0.0, 100.0),
            elapsed = elapsed,
        );
        let _ = err.flush();
        if matches!(phase, Phase::Done) {
            let _ = writeln!(err);
        }
        s.last_phase = Some(phase);
        s.last_shown_pct = pct_int;
    }
}

impl Sink for CliSink {
    fn phase(&self, phase: Phase) {
        let pct = if matches!(phase, Phase::Done) {
            100.0
        } else {
            0.0
        };
        self.render(phase, pct);
    }
    fn report_pct(&self, phase: Phase, pct: f64) {
        self.render(phase, pct);
    }
}

async fn run_transcribe(cli: Cli) -> Result<()> {
    if cli.inputs.is_empty() {
        return Err(api::Error::Config("no input files".into()));
    }

    let mut config = Config::load()?;
    if let Some(l) = cli.lang {
        config.language = l;
    }
    if let Some(m) = cli.model {
        config.model = m;
    }
    if let Some(t) = cli.threads {
        config.threads = t;
    }
    if let Some(d) = cli.device {
        config.device = d;
    }
    if let Some(e) = cli.engine {
        config.engine = e;
    }
    if cli.no_diarize {
        config.diarize = false;
    }
    if let Some(s) = cli.speakers {
        config.speakers = Some(s);
    }

    if matches!(config.device, Device::Cuda) && !cfg!(feature = "cuda") {
        let msg = "this build does not ship CUDA; \
             pass --device cpu, or install the CUDA build of WTranscriber";
        logfile::warn(&format!(
            "--device cuda requested on CPU-only build; falling back to CPU ({msg})",
        ));
        eprintln!("warning: {msg}; falling back to --device cpu");
        config.device = Device::Cpu;
    }

    logfile::info(&format!(
        "cli run: device={} engine={} model={} lang={} diarize={} speakers={:?} inputs={}",
        config.device.as_str(),
        config.engine.as_str(),
        config.model,
        config.language,
        config.diarize,
        config.speakers,
        cli.inputs.len(),
    ));

    for input in cli.inputs {
        if let Err(e) = transcribe_one(&input, &config, cli.no_cache, cli.rename).await {
            eprintln!("{}: {e}", input.display());
        }
    }
    Ok(())
}

async fn transcribe_one(input: &Path, config: &Config, no_cache: bool, rename: bool) -> Result<()> {
    let canonical = std::path::absolute(input)?;
    if !canonical.exists() {
        return Err(api::Error::Config(format!(
            "file not found: {}",
            canonical.display()
        )));
    }

    if no_cache {
        let speakers = config.speakers.unwrap_or(0);
        let key_params = api::transcript_cache::build_key_params(
            &canonical,
            &config.model,
            &config.language,
            speakers,
            !config.diarize,
            0,
            0,
        )?;
        let key = api::transcript_cache::compute_key(&key_params);
        let _ = api::transcript_cache::invalidate(&key);
        let _ = api::transcript_partial::clear(&key);
    }

    let job = Job {
        input: canonical.clone(),
        config: config.clone(),
    };

    eprintln!("transcribing: {}", canonical.display());
    let sink: Arc<dyn Sink> = Arc::new(CliSink::new(canonical.clone()));
    let transcript = api::transcribe_with_sink(&job, sink).await?;
    let dst = output_path(&canonical, &config.model);
    write_transcript(&dst, &transcript)?;
    println!("{}", dst.display());

    if rename && let Err(e) = auto_rename(&canonical, &transcript).await {
        eprintln!("auto-rename failed: {e}");
    }
    Ok(())
}

async fn auto_rename(audio: &Path, transcript: &Transcript) -> Result<()> {
    let t = transcript.clone();
    let suggestion = tokio::task::spawn_blocking(move || namer::suggest(&t, chrono::Local::now()))
        .await
        .map_err(|e| api::Error::Transcribe(format!("namer task: {e}")))??;
    let target = namer::rename_with_suggestion(audio, &suggestion)?;
    eprintln!("renamed: {} -> {}", audio.display(), target.display());
    Ok(())
}

fn output_path(input: &Path, model: &str) -> PathBuf {
    let parent = input.parent().unwrap_or_else(|| Path::new("."));
    let stem = input.file_stem().map_or_else(
        || "transcript".to_owned(),
        |s| s.to_string_lossy().into_owned(),
    );
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H%M%S");
    parent.join(format!("{stem}_{model}_{stamp}.json"))
}

fn write_transcript(path: &Path, transcript: &Transcript) -> Result<()> {
    let raw = serde_json::to_vec_pretty(transcript)?;
    std::fs::write(path, raw)?;
    Ok(())
}
