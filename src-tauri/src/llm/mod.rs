use std::{
    io::Write,
    path::{Path, PathBuf},
};

use crate::{
    error::{Error, Result},
    models::{Family, by_family, paths_for},
    paths,
    process::{find_executable, quiet_command},
};

#[derive(Debug, Clone)]
pub struct Runner {
    binary: PathBuf,
    model: PathBuf,
    threads: u32,
}

#[derive(Debug, Clone)]
pub struct Options {
    pub prompt: String,
    pub grammar: Option<String>,
    pub max_tokens: u32,
    pub temp: f32,
}

const fn binary_name() -> &'static str {
    if cfg!(windows) {
        "llama-cli.exe"
    } else {
        "llama-cli"
    }
}

fn find_binary() -> Result<PathBuf> {
    let name = binary_name();
    find_executable("WT_LLM_DIR", name, crate::runtimes::llama::find).map_err(|_| {
        Error::Transcribe(format!(
            "{name} not found (set WT_LLM_DIR or install llama.cpp)"
        ))
    })
}

fn first_installed_llm() -> Option<PathBuf> {
    if let Ok(env_path) = std::env::var("WT_LLM_MODEL") {
        let p = PathBuf::from(env_path);
        if p.exists() {
            return Some(p);
        }
    }
    for entry in by_family(Family::Llm) {
        if let Ok(p) = paths_for(entry)
            && let Some(first) = p.first()
            && first.exists()
        {
            return Some(first.clone());
        }
    }
    None
}

fn default_threads() -> u32 {
    u32::try_from(
        std::thread::available_parallelism()
            .map_or(4, std::num::NonZero::get)
            .min(6),
    )
    .unwrap_or(4)
}

impl Runner {
    pub fn new() -> Result<Self> {
        let binary = find_binary()?;
        let model = first_installed_llm()
            .ok_or_else(|| Error::Transcribe("no LLM installed (download one in Models)".into()))?;
        Ok(Self {
            binary,
            model,
            threads: default_threads(),
        })
    }

    pub fn generate(&self, opts: &Options) -> Result<String> {
        let max_tokens = if opts.max_tokens == 0 {
            128
        } else {
            opts.max_tokens
        };
        let temp = if opts.temp == 0.0 { 0.1 } else { opts.temp };

        let prompt_file = write_temp("wt-llm-prompt-", ".txt", &opts.prompt)?;
        let grammar_file = match opts.grammar.as_deref() {
            Some(g) => Some(write_temp("wt-llm-grammar-", ".gbnf", g)?),
            None => None,
        };

        let stdout = self.run_once(
            &prompt_file,
            grammar_file.as_deref(),
            max_tokens,
            temp,
            false,
        )?;
        if let Some(json) = last_balanced_json(&stdout) {
            return Ok(json);
        }

        let stdout_cpu = self.run_once(
            &prompt_file,
            grammar_file.as_deref(),
            max_tokens,
            temp,
            true,
        )?;
        last_balanced_json(&stdout_cpu).ok_or_else(|| {
            Error::Transcribe(format!(
                "no JSON object in llm output: {}",
                tail(&stdout_cpu, 400)
            ))
        })
    }

    fn run_once(
        &self,
        prompt: &Path,
        grammar: Option<&Path>,
        max_tokens: u32,
        temp: f32,
        cpu_only: bool,
    ) -> Result<String> {
        let mut cmd = quiet_command(self.binary.as_os_str());
        cmd.arg("-m")
            .arg(&self.model)
            .arg("-f")
            .arg(prompt)
            .args(["-n", &max_tokens.to_string()])
            .args(["-t", &self.threads.to_string()])
            .args(["--temp", &format!("{temp:.2}")])
            .args([
                "--no-display-prompt",
                "--no-conversation",
                "--single-turn",
                "--simple-io",
                "--no-warmup",
                "--log-disable",
            ]);
        if cpu_only {
            cmd.args(["-ngl", "0"]);
        }
        if let Some(g) = grammar {
            cmd.arg("--grammar-file").arg(g);
        }
        let out = cmd.output()?;
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

fn write_temp(prefix: &str, suffix: &str, content: &str) -> Result<PathBuf> {
    let dir = paths::cache_dir()?.join("llm");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!(
        "{prefix}{}-{}{suffix}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let mut file = std::fs::File::create(&path)?;
    file.write_all(content.as_bytes())?;
    Ok(path)
}

pub fn last_balanced_json(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut end: Option<usize> = None;
    for i in (0..bytes.len()).rev() {
        match bytes[i] {
            b'}' => {
                if depth == 0 {
                    end = Some(i);
                }
                depth += 1;
            }
            b'{' if depth > 0 => {
                depth -= 1;
                if depth == 0
                    && let Some(e) = end
                {
                    return Some(s[i..=e].to_owned());
                }
            }
            _ => {}
        }
    }
    None
}

fn tail(s: &str, n: usize) -> String {
    let s = s.trim();
    if s.len() > n {
        format!("...{}", &s[s.len() - n..])
    } else {
        s.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_last_json() {
        let out = "noise\nthen {\"a\":1} and later {\"topic\":\"hello-world\"}\nepilogue";
        assert_eq!(
            last_balanced_json(out),
            Some("{\"topic\":\"hello-world\"}".to_owned())
        );
    }

    #[test]
    fn returns_none_when_unbalanced() {
        assert_eq!(last_balanced_json("{not closed"), None);
    }

    #[test]
    fn handles_nested_objects() {
        let out = "{\"outer\":{\"inner\":1}}";
        assert_eq!(last_balanced_json(out), Some(out.to_owned()));
    }
}
