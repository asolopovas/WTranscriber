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

pub fn write_json_pretty(path: &Path, value: &serde_json::Value) -> Result<()> {
    let mut out = serde_json::to_string_pretty(value)?;
    out.push('\n');
    std::fs::write(path, out).with_context(|| format!("write {}", path.display()))
}

pub fn set_json_string(path: &Path, key: &str, value: &str) -> Result<()> {
    let mut json = read_json(path)?;
    json[key] = serde_json::Value::String(value.to_string());
    write_json_pretty(path, &json)
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
