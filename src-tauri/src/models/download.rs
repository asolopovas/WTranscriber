#![allow(clippy::cast_precision_loss)]

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

use crate::error::{Error, Result};

const MAX_READ_RETRIES: u32 = 8;
const MAX_DIAL_RETRIES: u32 = 30;
const REPORT_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ByteProgress {
    pub downloaded: u64,
    pub total: u64,
}

pub type Progress = ByteProgress;

#[allow(clippy::too_many_lines)]
pub async fn download_file(
    dst: &Path,
    url: &str,
    expected_sha256: Option<&str>,
    on_progress: &mut (dyn FnMut(Progress) + Send),
) -> Result<()> {
    if let Some(parent) = dst.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let tmp: PathBuf = dst.with_extension(format!(
        "{}.part",
        dst.extension().and_then(|s| s.to_str()).unwrap_or("")
    ));

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| Error::Other(e.into()))?;

    let mut last_err: Option<Error> = None;
    let mut read_attempt = 0u32;
    let mut dial_attempt = 0u32;

    while read_attempt < MAX_READ_RETRIES && dial_attempt < MAX_DIAL_RETRIES {
        let offset = tokio::fs::metadata(&tmp).await.map_or(0, |m| m.len());

        let mut req = client.get(url);
        if offset > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={offset}-"));
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                dial_attempt += 1;
                last_err = Some(Error::Other(e.into()));
                tokio::time::sleep(backoff(dial_attempt)).await;
                continue;
            }
        };

        let status = resp.status();
        if !(status.is_success() || status == reqwest::StatusCode::PARTIAL_CONTENT) {
            return Err(Error::Other(anyhow::anyhow!("HTTP {status}")));
        }

        let resume = status == reqwest::StatusCode::PARTIAL_CONTENT;
        let mut written = if resume { offset } else { 0 };
        if !resume && tokio::fs::try_exists(&tmp).await.unwrap_or(false) {
            tokio::fs::remove_file(&tmp).await?;
        }

        let total = resp.content_length().unwrap_or(0) + written;
        on_progress(Progress {
            downloaded: written,
            total,
        });

        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(resume)
            .truncate(!resume)
            .open(&tmp)
            .await?;
        if resume {
            file.seek(std::io::SeekFrom::End(0)).await?;
        }

        let mut stream = resp.bytes_stream();
        let mut last_report = Instant::now();
        let mut copy_err: Option<Error> = None;

        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(bytes) => {
                    if let Err(e) = file.write_all(&bytes).await {
                        copy_err = Some(Error::Io(e));
                        break;
                    }
                    written += bytes.len() as u64;
                    if last_report.elapsed() >= REPORT_INTERVAL {
                        on_progress(Progress {
                            downloaded: written,
                            total,
                        });
                        last_report = Instant::now();
                    }
                }
                Err(e) => {
                    copy_err = Some(Error::Other(e.into()));
                    break;
                }
            }
        }
        file.flush().await?;
        drop(file);

        match copy_err {
            None => {
                on_progress(Progress {
                    downloaded: written,
                    total,
                });
                if let Some(expected) = expected_sha256
                    && !expected.is_empty()
                {
                    verify_sha256(&tmp, expected).await?;
                }
                tokio::fs::rename(&tmp, dst).await?;
                return Ok(());
            }
            Some(e) => {
                read_attempt += 1;
                last_err = Some(e);
                tokio::time::sleep(backoff(read_attempt)).await;
            }
        }
    }

    Err(last_err.unwrap_or_else(|| Error::Other(anyhow::anyhow!("download failed"))))
}

fn backoff(attempt: u32) -> Duration {
    Duration::from_secs(u64::from(attempt.saturating_mul(2))).min(Duration::from_secs(15))
}

async fn verify_sha256(path: &Path, expected: &str) -> Result<()> {
    let path = path.to_owned();
    let expected = expected.to_owned();
    tokio::task::spawn_blocking(move || -> Result<()> {
        use std::io::Read as _;
        let mut file = std::fs::File::open(&path)?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        let actual = hex::encode(hasher.finalize());
        if !actual.eq_ignore_ascii_case(&expected) {
            return Err(Error::Other(anyhow::anyhow!(
                "sha256 mismatch: expected {expected}, got {actual}"
            )));
        }
        Ok(())
    })
    .await
    .map_err(|e| Error::Other(anyhow::anyhow!("verify task: {e}")))?
}
