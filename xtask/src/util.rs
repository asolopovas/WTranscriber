use anyhow::{Context, Result, bail};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask manifest has parent")
        .to_path_buf()
}

pub fn is_windows() -> bool {
    cfg!(target_os = "windows")
}

pub fn exe(name: &str) -> String {
    if is_windows() {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

pub fn pkg_version() -> Result<String> {
    let p = root().join("package.json");
    let v = read_json(&p)?;
    Ok(v["version"]
        .as_str()
        .context("package.json missing .version")?
        .to_string())
}

pub fn read_json(path: &Path) -> Result<serde_json::Value> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))
}

pub fn set_json_string(path: &Path, key: &str, value: &str) -> Result<()> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let needle = format!("\"{key}\"");
    let key_pos = raw
        .find(&needle)
        .with_context(|| format!("{} has no key {key}", path.display()))?;
    let rest = &raw[key_pos + needle.len()..];
    let colon = rest
        .find(':')
        .with_context(|| format!("{} key {key} has no colon", path.display()))?;
    let value_part = &rest[colon + 1..];
    let open = value_part
        .find('"')
        .with_context(|| format!("{} key {key} has no string value", path.display()))?;
    let close = value_part[open + 1..]
        .find('"')
        .with_context(|| format!("{} key {key} value is unterminated", path.display()))?;
    let start = key_pos + needle.len() + colon + 1 + open + 1;
    let end = start + close;
    let out = format!("{}{}{}", &raw[..start], value, &raw[end..]);
    let parsed: serde_json::Value = serde_json::from_str(&out)
        .with_context(|| format!("{} invalid after edit", path.display()))?;
    if parsed[key].as_str() != Some(value) {
        bail!(
            "{} key {key} did not take value {value} after edit",
            path.display()
        );
    }
    std::fs::write(path, out).with_context(|| format!("write {}", path.display()))
}

pub fn git_short_sha() -> Result<String> {
    capture("git", &["rev-parse", "--short", "HEAD"])
}

pub fn git_branch() -> Result<String> {
    let b = capture("git", &["rev-parse", "--abbrev-ref", "HEAD"])?;
    Ok(if b == "HEAD" { "main".into() } else { b })
}

pub fn capture(prog: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(prog)
        .args(args)
        .current_dir(root())
        .output()
        .with_context(|| format!("spawn {prog}"))?;
    if !out.status.success() {
        bail!(
            "{} {:?} failed: {}",
            prog,
            args,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn sh(prog: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(prog)
        .args(args)
        .current_dir(root())
        .status()
        .with_context(|| format!("spawn {prog}"))?;
    if !status.success() {
        bail!("{} {:?} exited with {:?}", prog, args, status.code());
    }
    Ok(())
}

pub fn sh_in(cwd: &Path, prog: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(prog)
        .args(args)
        .current_dir(cwd)
        .status()
        .with_context(|| format!("spawn {prog}"))?;
    if !status.success() {
        bail!("{} {:?} exited with {:?}", prog, args, status.code());
    }
    Ok(())
}

pub type SharedOut = Arc<Mutex<()>>;

pub fn shared_out() -> SharedOut {
    Arc::new(Mutex::new(()))
}

pub fn parallel_jobs() -> usize {
    std::env::var("WT_BUILD_JOBS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or_else(|| thread::available_parallelism().map_or(1, std::num::NonZero::get))
}

pub fn parallel_build_env(jobs: usize) -> Vec<(String, String)> {
    let jobs = jobs.to_string();
    vec![
        ("CARGO_BUILD_JOBS".into(), jobs.clone()),
        ("CMAKE_BUILD_PARALLEL_LEVEL".into(), jobs.clone()),
        ("MAKEFLAGS".into(), format!("-j{jobs}")),
        ("GRADLE_OPTS".into(), gradle_opts_with_workers(&jobs)),
    ]
}

pub fn configure_parallel_build_env(jobs: usize) {
    for (key, value) in parallel_build_env(jobs) {
        unsafe { std::env::set_var(key, value) };
    }
}

fn gradle_opts_with_workers(jobs: &str) -> String {
    let flag = format!("-Dorg.gradle.workers.max={jobs}");
    let Ok(existing) = std::env::var("GRADLE_OPTS") else {
        return flag;
    };
    let trimmed = existing.trim();
    if trimmed
        .split_whitespace()
        .any(|part| part.starts_with("-Dorg.gradle.workers.max="))
    {
        trimmed.to_string()
    } else if trimmed.is_empty() {
        flag
    } else {
        format!("{flag} {trimmed}")
    }
}

pub fn run_streamed(
    tag: &str,
    prog: &str,
    args: &[&str],
    extra_env: &[(&str, &str)],
    out_lock: &SharedOut,
) -> Result<i32> {
    let prefix = format!("[{tag}] ");
    let mut cmd = Command::new(prog);
    cmd.args(args)
        .current_dir(root())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().with_context(|| format!("spawn {prog}"))?;
    let stdout = child.stdout.take().context("no stdout")?;
    let stderr = child.stderr.take().context("no stderr")?;
    let prefix_o = prefix.clone();
    let lock_o = out_lock.clone();
    let h_out = thread::spawn(move || forward_lines(stdout, &prefix_o, &lock_o));
    let prefix_e = prefix.clone();
    let lock_e = out_lock.clone();
    let h_err = thread::spawn(move || forward_lines(stderr, &prefix_e, &lock_e));
    let status = child.wait()?;
    let _ = h_out.join();
    let _ = h_err.join();
    Ok(status.code().unwrap_or(1))
}

fn forward_lines<R: std::io::Read>(reader: R, prefix: &str, lock: &SharedOut) {
    let r = BufReader::new(reader);
    for line in r.lines().map_while(|l| l.ok()) {
        let _g = lock.lock().unwrap();
        let mut stdout = std::io::stdout().lock();
        let _ = writeln!(stdout, "{prefix}{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::set_json_string;

    #[test]
    fn set_json_string_edits_in_place() {
        let raw = "{\n  \"name\": \"app\",\n  \"version\": \"0.1.13\",\n  \"bundle\": {\n    \"targets\": [\"nsis\", \"deb\"]\n  }\n}\n";
        let path = std::env::temp_dir().join(format!("xtask-set-json-{}.json", std::process::id()));
        std::fs::write(&path, raw).unwrap();
        set_json_string(&path, "version", "0.1.14").unwrap();
        let out = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(out, raw.replace("0.1.13", "0.1.14"));
    }
}
