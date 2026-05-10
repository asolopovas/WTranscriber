use crate::{
    error::{Error, Result},
    logfile, namer,
    namer::Suggestion,
    transcriber::Transcript,
};

#[tauri::command]
pub async fn suggest_filename(transcript: Transcript) -> Result<Suggestion> {
    let utterances = transcript.utterances.len();
    let t0 = std::time::Instant::now();
    logfile::info(&format!(
        "auto-rename: suggesting from {utterances} utterances"
    ));
    let result =
        tokio::task::spawn_blocking(move || namer::suggest(&transcript, chrono::Local::now()))
            .await
            .map_err(|e| Error::Transcribe(format!("task: {e}")))?;
    match &result {
        Ok(s) => logfile::info(&format!(
            "auto-rename: suggested '{}_{}' in {:.2}s",
            s.topic,
            s.stamp,
            t0.elapsed().as_secs_f64(),
        )),
        Err(e) => logfile::warn(&format!("auto-rename failed: {e}")),
    }
    result
}
