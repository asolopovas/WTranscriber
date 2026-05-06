#![allow(clippy::needless_pass_by_value)]

use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::{Parser, Subcommand, ValueEnum};
use wtranscriber_lib::{
    api::{
        self, Config, Device, Engine, Family, FileProgress, Job, ModelStatus, Result, Transcript,
    },
    namer,
};

#[derive(Parser, Debug)]
#[command(
    name = "wt",
    version,
    about = "Audio transcription CLI (sherpa-onnx + pyannote)",
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    inputs: Vec<PathBuf>,

    #[arg(short, long)]
    lang: Option<String>,

    #[arg(short, long)]
    model: Option<String>,

    #[arg(short, long)]
    threads: Option<u32>,

    #[arg(long)]
    speakers: Option<u32>,

    #[arg(long)]
    no_diarize: bool,

    #[arg(long, value_enum)]
    device: Option<DeviceArg>,

    #[arg(long, value_enum)]
    engine: Option<EngineArg>,

    #[arg(long)]
    no_cache: bool,

    #[arg(long)]
    rename: bool,
}

#[derive(Subcommand, Debug)]
enum Command {
    Models {
        #[command(subcommand)]
        action: ModelsAction,
    },
}

#[derive(Subcommand, Debug)]
enum ModelsAction {
    List,
    Install { id: String },
    Status { id: String },
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum DeviceArg {
    Cpu,
    Cuda,
}

impl From<DeviceArg> for Device {
    fn from(d: DeviceArg) -> Self {
        match d {
            DeviceArg::Cpu => Self::Cpu,
            DeviceArg::Cuda => Self::Cuda,
        }
    }
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum EngineArg {
    WhisperOnnx,
    Zipformer,
    Parakeet,
    Canary,
    NemoCtc,
}

impl From<EngineArg> for Engine {
    fn from(e: EngineArg) -> Self {
        match e {
            EngineArg::WhisperOnnx => Self::WhisperOnnx,
            EngineArg::Zipformer => Self::Zipformer,
            EngineArg::Parakeet => Self::Parakeet,
            EngineArg::Canary => Self::Canary,
            EngineArg::NemoCtc => Self::NemoCtc,
        }
    }
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
                let family = match m.family {
                    Family::Asr => "asr",
                    Family::Diarizer => "diarizer",
                    Family::Llm => "llm",
                };
                println!(
                    "{:<32} {:<10} {:<14} {:>8} MB  {}",
                    m.id,
                    family,
                    status_label(m.status),
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
            println!("{}", status_label(s));
        }
    }
    Ok(())
}

const fn status_label(s: ModelStatus) -> &'static str {
    match s {
        ModelStatus::Installed => "installed",
        ModelStatus::Downloading => "downloading",
        ModelStatus::NotInstalled => "not_installed",
    }
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
        config.device = d.into();
    }
    if let Some(e) = cli.engine {
        config.engine = e.into();
    }
    if cli.no_diarize {
        config.diarize = false;
    }
    if let Some(s) = cli.speakers {
        config.speakers = Some(s);
    }

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
    let transcript = api::transcribe(&job).await?;
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
