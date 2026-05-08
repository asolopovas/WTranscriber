use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Command,
};

use crate::error::{Error, Result};

#[must_use]
pub fn quiet_command(program: impl AsRef<OsStr>) -> Command {
    let mut cmd = Command::new(program);
    silence(&mut cmd);
    cmd
}

#[cfg(windows)]
pub fn silence(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
pub fn silence(_cmd: &mut Command) {}

pub fn find_executable<F>(env_dir: &str, name: &str, extra: F) -> Result<PathBuf>
where
    F: FnOnce() -> Option<PathBuf>,
{
    if let Ok(dir) = std::env::var(env_dir) {
        let p = Path::new(&dir).join(name);
        if p.exists() {
            return Ok(p);
        }
    }
    if let Some(p) = extra() {
        return Ok(p);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let p = dir.join(name);
        if p.exists() {
            return Ok(p);
        }
    }
    if let Ok(p) = which::which(name) {
        return Ok(p);
    }
    Err(Error::Transcribe(format!("{name} not found")))
}

pub fn walk_for_file<P>(dir: &Path, depth: usize, predicate: P) -> Option<PathBuf>
where
    P: Fn(&Path) -> bool + Copy,
{
    if depth == 0 {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for e in entries.flatten() {
        let path = e.path();
        let Ok(ty) = e.file_type() else { continue };
        if predicate(&path) {
            return Some(path);
        }
        if ty.is_dir() {
            subdirs.push(path);
        }
    }
    for s in subdirs {
        if let Some(p) = walk_for_file(&s, depth - 1, predicate) {
            return Some(p);
        }
    }
    None
}
