#![allow(
    clippy::significant_drop_tightening,
    clippy::too_many_lines,
    clippy::items_after_statements
)]

use std::sync::{Mutex, OnceLock};

use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

use crate::{
    config::Config,
    error::{Error, Result},
    models,
    transcriber::{Segment, Token},
};

struct CtxCell {
    model_path: String,
    _ctx: WhisperContext,
    state: Mutex<WhisperState>,
}

static CTX: OnceLock<Mutex<Option<CtxCell>>> = OnceLock::new();

fn ctx_slot() -> &'static Mutex<Option<CtxCell>> {
    CTX.get_or_init(|| Mutex::new(None))
}

fn resolve_model_path(model_id: &str) -> Result<std::path::PathBuf> {
    let entry = models::by_id(model_id).ok_or_else(|| {
        Error::Config(format!(
            "unknown whisper-cpp model id `{model_id}` (run `wt models list`)"
        ))
    })?;
    let files = models::paths_for(entry)?;
    let path = files
        .into_iter()
        .find(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("bin"))
        })
        .ok_or_else(|| {
            Error::Transcribe(format!(
                "whisper-cpp model `{model_id}` has no .bin file in catalog"
            ))
        })?;
    if !path.exists() {
        return Err(Error::Transcribe(format!(
            "whisper-cpp model file missing: {} (install via `wt models install {model_id}`)",
            path.display(),
        )));
    }
    Ok(path)
}

fn ensure_state(model_path: &std::path::Path) -> Result<()> {
    let mut slot = ctx_slot()
        .lock()
        .map_err(|e| Error::Transcribe(format!("whisper-cpp ctx lock: {e}")))?;
    let model_str = model_path
        .to_str()
        .ok_or_else(|| Error::Config("whisper-cpp model path is not UTF-8".into()))?
        .to_owned();
    if slot.as_ref().is_some_and(|c| c.model_path == model_str) {
        return Ok(());
    }
    let ctx = WhisperContext::new_with_params(&model_str, WhisperContextParameters::default())
        .map_err(|e| Error::Transcribe(format!("whisper-cpp init {model_str}: {e}")))?;
    let state = ctx
        .create_state()
        .map_err(|e| Error::Transcribe(format!("whisper-cpp state: {e}")))?;
    *slot = Some(CtxCell {
        model_path: model_str,
        _ctx: ctx,
        state: Mutex::new(state),
    });
    Ok(())
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
const fn t_centisec_to_ms(t: i64) -> u64 {
    if t < 0 {
        return 0;
    }
    (t as u64).saturating_mul(10)
}

pub fn run(
    samples: &[f32],
    audio_dur_sec: f64,
    config: &Config,
    on_progress: &mut dyn FnMut(f64),
    cancelled: &dyn Fn() -> bool,
) -> Result<(Vec<Segment>, String, f64)> {
    let model_path = resolve_model_path(&config.model)?;
    ensure_state(&model_path)?;
    let slot = ctx_slot()
        .lock()
        .map_err(|e| Error::Transcribe(format!("whisper-cpp slot lock: {e}")))?;
    let cell = slot
        .as_ref()
        .ok_or_else(|| Error::Transcribe("whisper-cpp ctx not initialised".into()))?;
    let mut state = cell
        .state
        .lock()
        .map_err(|e| Error::Transcribe(format!("whisper-cpp state lock: {e}")))?;

    let lang = config.language.trim();
    let lang_arg = (!lang.is_empty() && !lang.eq_ignore_ascii_case("auto")).then_some(lang);

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(lang_arg);
    params.set_token_timestamps(true);
    params.set_split_on_word(true);
    params.set_max_len(1);
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    params.set_n_threads(i32::try_from(config.threads).unwrap_or(4));
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    params.set_print_timestamps(false);
    params.set_translate(false);
    params.set_single_segment(false);

    if cancelled() {
        return Err(Error::Cancelled);
    }
    let t0 = std::time::Instant::now();
    state
        .full(params, samples)
        .map_err(|e| Error::Transcribe(format!("whisper-cpp full: {e}")))?;
    on_progress(100.0);
    let elapsed = t0.elapsed().as_secs_f64();
    if cancelled() {
        return Err(Error::Cancelled);
    }

    let n = state.full_n_segments();
    let mut segs: Vec<Segment> = Vec::new();
    let mut current: Option<Segment> = None;
    fn flush(current: &mut Option<Segment>, segs: &mut Vec<Segment>) {
        if let Some(seg) = current.take() {
            if !seg.text.trim().is_empty() {
                segs.push(seg);
            }
        }
    }

    for i in 0..n {
        let Some(seg_handle) = state.get_segment(i) else {
            continue;
        };
        let raw = seg_handle
            .to_str_lossy()
            .map_err(|e| Error::Transcribe(format!("whisper-cpp seg text {i}: {e}")))?;
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

    let detected_idx = state.full_lang_id_from_state();
    let detected = if detected_idx >= 0 {
        whisper_rs::get_lang_str(detected_idx).map_or_else(|| lang.to_owned(), str::to_owned)
    } else {
        lang.to_owned()
    };

    let rtf = if audio_dur_sec > 0.0 {
        elapsed / audio_dur_sec
    } else {
        0.0
    };
    Ok((segs, detected, rtf))
}
