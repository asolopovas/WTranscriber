use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::Mutex,
};

use chrono::Local;

use crate::{error::Result, paths};

const MAX_BYTES: u64 = 5 * 1024 * 1024;
const MAX_BACKUPS: usize = 48;

static LOG: Mutex<Option<File>> = Mutex::new(None);

pub fn log_path() -> Result<PathBuf> {
    Ok(paths::data_dir()?.join("wt.log"))
}

fn open() -> Result<File> {
    let path = log_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(OpenOptions::new().create(true).append(true).open(path)?)
}

fn rotate_if_needed() -> Result<()> {
    let path = log_path()?;
    let Ok(meta) = fs::metadata(&path) else {
        return Ok(());
    };
    if meta.len() < MAX_BYTES {
        return Ok(());
    }
    let stamp = Local::now().format("%Y%m%d-%H%M%S");
    let dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let archive = dir.join(format!("wt-{stamp}.log"));
    let _ = fs::rename(&path, &archive);
    prune_archives();
    Ok(())
}

fn prune_archives() {
    let Ok(path) = log_path() else { return };
    let Some(dir) = path.parent() else { return };
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut archives: Vec<_> = entries
        .flatten()
        .filter(|e| {
            e.file_name().to_string_lossy().starts_with("wt-")
                && e.file_name().to_string_lossy().ends_with(".log")
        })
        .collect();
    archives.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
    while archives.len() > MAX_BACKUPS {
        if let Some(old) = archives.first() {
            let _ = fs::remove_file(old.path());
        }
        archives.remove(0);
    }
}

fn write_line(level: &str, msg: &str) {
    let _ = (|| -> Result<()> {
        rotate_if_needed()?;
        let mut guard = LOG
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if guard.is_none() {
            *guard = Some(open()?);
        }
        if let Some(f) = guard.as_mut() {
            let stamp = Local::now().format("%H:%M:%S");
            let _ = writeln!(f, "{stamp} {level:<5} {msg}");
            let _ = f.flush();
        }
        drop(guard);
        Ok(())
    })();
}

pub fn info(msg: &str) {
    eprintln!("INFO  {msg}");
    write_line("INFO", msg);
}

pub fn warn(msg: &str) {
    eprintln!("WARN  {msg}");
    write_line("WARN", msg);
}

pub fn error(msg: &str) {
    eprintln!("ERROR {msg}");
    write_line("ERROR", msg);
}

pub fn process_start(label: &str) {
    let stamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    write_line("PROC", &format!("\n----- {stamp}  {label} started"));
}

pub fn process_end(label: &str, outcome: &str, details: &str) {
    let stamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    let mut line = format!("----- {stamp}  {label} {outcome}");
    if !details.is_empty() {
        line.push_str(" — ");
        line.push_str(details);
    }
    write_line("PROC", &line);
}

pub fn read_tail(max_bytes: u64) -> String {
    let Ok(path) = log_path() else {
        return String::new();
    };
    let Ok(meta) = fs::metadata(&path) else {
        return String::new();
    };
    let Ok(mut f) = File::open(&path) else {
        return String::new();
    };
    let off = meta.len().saturating_sub(max_bytes);
    if f.seek(SeekFrom::Start(off)).is_err() {
        return String::new();
    }
    let mut buf = String::new();
    let _ = f.read_to_string(&mut buf);
    buf
}

pub fn clear() -> Result<()> {
    {
        let mut guard = LOG
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = None;
    }
    let path = log_path()?;
    if let Some(parent) = path.parent() {
        if let Ok(entries) = fs::read_dir(parent) {
            for e in entries.flatten() {
                let n = e.file_name();
                let s = n.to_string_lossy();
                if s.starts_with("wt-") && s.ends_with(".log") {
                    let _ = fs::remove_file(e.path());
                }
            }
        }
    }
    let _ = fs::remove_file(&path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_temp_paths(test: impl FnOnce(&std::path::Path)) {
        let _guard = crate::paths::PATHS_TEST_LOCK.lock().unwrap();
        crate::paths::clear_test_overrides();
        let dir = tempfile::tempdir().unwrap();
        crate::paths::init(
            dir.path().join("config"),
            dir.path().join("data"),
            dir.path().join("cache"),
        );
        clear().unwrap();
        test(dir.path());
        clear().unwrap();
        crate::paths::clear_test_overrides();
    }

    #[test]
    fn read_tail_returns_recent_log_content() {
        with_temp_paths(|_| {
            write_line("INFO", "first line");
            write_line("WARN", "second line");

            let tail = read_tail(32);

            assert!(tail.contains("second line"));
        });
    }

    #[test]
    fn clear_removes_active_log_and_archives() {
        with_temp_paths(|_| {
            write_line("INFO", "active");
            let path = log_path().unwrap();
            let archive = path.parent().unwrap().join("wt-20000101-000000.log");
            std::fs::write(&archive, b"old").unwrap();

            clear().unwrap();

            assert!(!path.exists());
            assert!(!archive.exists());
        });
    }
}
