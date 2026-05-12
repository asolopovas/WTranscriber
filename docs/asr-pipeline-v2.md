# ASR + diarization pipeline v2

State after **wave 1** (this branch, already shipped):

- `wt --model X` derives `config.engine` from the catalog entry (no more silent `whisper-onnx` engine over a Parakeet model).
- `wt --diarizer {sortformer-onnx|nemo|titanet}` selects the diarizer backend per run.
- `wt --no-auto-route` opts out of language-aware model selection.
- `api::route_model_for_lang(lang)` maps a language code to the best installed ASR (`ru`→GigaAM, parakeet's 25 EU langs→Parakeet, else→sherpa-whisper-turbo).
- CLI warns when `whisper-onnx` is paired with `diarize=true`.
- Default `config.language` is now `"auto"` for fresh installs.

Wave 1 is enough to make English / Russian / EU multi-speaker transcription correct out of the box: those languages route to Parakeet or GigaAM, both of which emit token-level timestamps and merge correctly with any of the three diarizers.

What wave 1 does **not** fix: the multilingual fallback path (`zh / ja / ko / ar / hi / tr / ...`) still lands on the bundled sherpa-onnx-whisper-turbo export, which has no cross-attention and therefore collapses diarized speakers on long ASR spans. There is also no real language probe — if `--lang` is omitted on a fresh install the router keeps the current model.

Wave 2 closes both gaps.

## Wave 2 — concrete plan

### 1. Replace the multilingual fallback with whisper.cpp (`whisper-rs`)

| Aspect              | Decision                                                                                 | Notes                                                                                                                                         |
| ------------------- | ---------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| Crate               | `whisper-rs = { version = "0.16", default-features = false }`                            | Optional CUDA via `features = ["cuda"]` once nvcc is on PATH. Pure CPU works without it.                                                      |
| Cargo.toml location | Inside `[target.'cfg(not(any(target_os = "android", target_os = "ios")))'.dependencies]` | Same gate as `parakeet-rs`. Android keeps Parakeet-only.                                                                                      |
| New engine variant  | `Engine::WhisperCpp` in `src-tauri/src/config.rs`                                        | serde tag `"whisper-cpp"`; mirror in `src/types.ts`.                                                                                          |
| New engine module   | `src-tauri/src/engine/whisper_cpp.rs`                                                    | Exposes `run(samples, audio_dur_sec, config, on_progress, cancelled) -> Result<(Vec<Segment>, String, f64)>` matching `engine::whisper::run`. |
| In-process flag     | Add `Engine::WhisperCpp` to the in-process match in `engine/mod.rs::use_in_process`      | No subprocess fallback needed — the crate is in-process by design.                                                                            |
| VAD                 | Reuse existing `vad::model` Silero VAD path that already feeds `engine::whisper::run`    | Pass VAD regions into `WhisperState::full` per chunk to keep chunk lengths ≤30 s.                                                             |
| Word timestamps     | `FullParams::set_token_timestamps(true).set_split_on_word(true).set_max_len(1)`          | Produces one whisper-cpp "segment" per word → fan into `Segment.tokens`.                                                                      |
| Resampling          | `samples` arrives at 16 kHz mono from `audio/decode.rs`                                  | No conversion needed.                                                                                                                         |
| Cancellation        | `whisper-rs` exposes `set_abort_callback`; wire it to the `cancelled` closure.           |                                                                                                                                               |
| Threading           | `params.set_n_threads(config.threads as i32)`                                            | Same convention as other engines.                                                                                                             |

#### Minimal engine sketch

```rust
// src-tauri/src/engine/whisper_cpp.rs
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters,
};

use crate::{
    config::Config,
    error::{Error, Result},
    models,
    transcriber::Segment,
};

pub fn run(
    samples: &[f32],
    audio_dur_sec: f64,
    config: &Config,
    on_progress: &mut dyn FnMut(f64),
    cancelled: &dyn Fn() -> bool,
) -> Result<(Vec<Segment>, String, f64)> {
    let model_path = models::paths_for(
        models::by_id(&config.model)
            .ok_or_else(|| Error::Config(format!("unknown model {}", config.model)))?,
    )?
    .into_iter()
    .next()
    .ok_or_else(|| Error::Config("whisper-cpp model file missing".into()))?;

    let ctx = WhisperContext::new_with_params(
        model_path.to_str().ok_or_else(|| Error::Config("non-utf8 model path".into()))?,
        WhisperContextParameters::default(),
    )
    .map_err(|e| Error::Transcribe(format!("whisper-cpp init: {e}")))?;
    let mut state = ctx
        .create_state()
        .map_err(|e| Error::Transcribe(format!("whisper-cpp state: {e}")))?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    let lang_override = (!config.language.is_empty() && config.language != "auto")
        .then(|| config.language.as_str());
    params.set_language(lang_override);
    params.set_token_timestamps(true);
    params.set_split_on_word(true);
    params.set_max_len(1);
    params.set_n_threads(config.threads as i32);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);

    let t0 = std::time::Instant::now();
    state
        .full(params, samples)
        .map_err(|e| Error::Transcribe(format!("whisper-cpp full: {e}")))?;
    let elapsed = t0.elapsed().as_secs_f64();
    let rtf = if audio_dur_sec > 0.0 { elapsed / audio_dur_sec } else { 0.0 };
    on_progress(100.0);
    if cancelled() {
        return Err(Error::Cancelled);
    }

    let n = state
        .full_n_segments()
        .map_err(|e| Error::Transcribe(format!("n_segments: {e}")))?;
    let mut segs = Vec::with_capacity(n as usize);
    for i in 0..n {
        let text = state
            .full_get_segment_text(i)
            .map_err(|e| Error::Transcribe(format!("seg text {i}: {e}")))?;
        let t0 = state.full_get_segment_t0(i).unwrap_or(0) as u64 * 10; // whisper-cpp returns centiseconds → ms
        let t1 = state.full_get_segment_t1(i).unwrap_or(0) as u64 * 10;
        segs.push(Segment {
            text,
            start_ms: t0,
            end_ms: t1,
            tokens: Vec::new(), // expand by walking full_get_token_data if needed
        });
    }
    let detected = state.full_lang_id().ok().and_then(|i| state.full_lang_id_to_str(i).ok()).unwrap_or_default();
    Ok((segs, detected, rtf))
}
```

#### Catalog entry

```rust
// src-tauri/src/models/catalog_data.rs — push into the ASR section.
Entry {
    id: "whisper-cpp-large-v3-turbo-q8".into(),
    family: Family::Asr,
    engine: "whisper-cpp".into(),
    display_name: "Whisper large-v3-turbo (whisper.cpp, Q8_0, multilingual)".into(),
    description: "OpenAI Whisper large-v3-turbo via whisper.cpp with native word \
                  timestamps. 99 languages. ~874 MB.".into(),
    languages: vec!["auto","en","de","fr","es","it","pt","nl","pl","ru","uk","zh","ja","ko","ar","tr","hi"]
        .into_iter().map(String::from).collect(),
    size_bytes: 874_000_000,
    default_active: false,
    android_default: false,
    desktop_only: true,
    files: vec![FileSpec {
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q8_0.bin".into(),
        rel_path: "whisper-cpp-large-v3-turbo-q8/ggml-large-v3-turbo-q8_0.bin".into(),
        size_bytes: 874_000_000,
        sha256: "<fill once downloaded>".into(),
    }],
},
```

`sha256` must be filled by downloading the file once and hashing it; the installer’s integrity check otherwise rejects the model.

#### Routing change

```rust
// src-tauri/src/api.rs — replace "sherpa-whisper-turbo" with the cpp variant.
let candidates: &[&str] = match lang_code {
    "ru" => &["gigaam-v3-ru", "parakeet-tdt-0.6b-v3-int8", "whisper-cpp-large-v3-turbo-q8", "sherpa-whisper-turbo"],
    "bg" | "hr" | "cs" | … => &["parakeet-tdt-0.6b-v3-int8", "whisper-cpp-large-v3-turbo-q8", "sherpa-whisper-turbo"],
    _ => &["whisper-cpp-large-v3-turbo-q8", "sherpa-whisper-turbo"],
};
```

Keep `sherpa-whisper-turbo` as the last-resort fallback so users who have only the legacy model installed still get a working pipeline.

### 2. Silero LangID probe

| Aspect         | Decision                                                                                                                           |
| -------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| Model          | `deepghs/silero-lang95-onnx/lang_classifier_95.onnx` (~16 MB)                                                                      |
| Crate          | Reuse the `ort` crate already pulled by `parakeet-rs`                                                                              |
| Module         | `src-tauri/src/lang_id.rs`                                                                                                         |
| Catalog family | Add `Family::LangId` (or reuse `Family::Diarizer` with a tag) and a single auto-installed entry                                    |
| When run       | In `bin/wt.rs::run_transcribe`, after merging CLI into config, _only_ if `!model_explicit && (config.language is empty or "auto")` |
| Cost           | ~10–15 ms on CPU for the first 3 s of voiced audio; cache per input path                                                           |

#### Probe sketch

```rust
// src-tauri/src/lang_id.rs
use ort::{session::Session, value::Value};
use ndarray::Array2;

pub fn detect(samples_16k: &[f32], model_path: &std::path::Path) -> crate::error::Result<String> {
    let session = Session::builder()?.commit_from_file(model_path)?;
    let n = samples_16k.len();
    let arr = Array2::from_shape_vec((1, n), samples_16k.to_vec())?;
    let input = Value::from_array(arr)?;
    let outputs = session.run(ort::inputs!["input" => input])?;
    let logits = outputs[0].try_extract_array::<f32>()?;
    let (idx, _) = logits
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .unwrap();
    Ok(SILERO_LANG_CODES[idx].to_owned())
}
```

`SILERO_LANG_CODES` is a static slice mirroring the order in the model README. Feed only voiced audio — run Silero VAD first, take the first ~3 s of voiced frames; otherwise the classifier returns `en` on near-silence.

#### CLI wiring

```rust
// after the lang_known check, in the `else` branch:
let probe = audio::decode::sample_window(&canonical, 0.0, 5.0)?; // first 5 s, 16 kHz mono
let detected = lang_id::detect(&probe, &lang_id_model_path)?;
logfile::info(&format!("silero-langid -> {detected}"));
if let Some((id, eng)) = api::route_model_for_lang(&detected) {
    config.model = id;
    config.engine = eng;
}
config.language = detected;
```

`audio::decode::sample_window` is a small helper to add to `audio/decode.rs` — slice the symphonia-decoded buffer at sample boundaries (no ffmpeg needed).

### 3. Android-specific recipe

Android keeps its single-model pipeline:

| Stage       | Choice                                                                 | Notes                                                                      |
| ----------- | ---------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| ASR         | `parakeet-tdt-0.6b-v3-int8`                                            | 640 MB int8, covers 25 EU languages incl. Russian, native word timestamps. |
| LangID      | Skip                                                                   | Parakeet has built-in language auto-detect across its set.                 |
| VAD         | Silero VAD (already wired)                                             |                                                                            |
| Diarization | `sortformer-v2-onnx-4spk`                                              | ~115 MB int8, no Python.                                                   |
| Fallback    | None — non-EU langs not supported on mobile; surface a friendly error. |                                                                            |

Concretely: no Wave 2 changes are needed on Android; verify by building `just android` and running the existing samples. The whisper-cpp model and Silero LangID are `desktop_only = true` in the catalog.

### 4. Verification matrix (re-run after Wave 2)

```
wt --no-cache audio_30s_4speakers.m4a            # routes to parakeet via langid
wt --lang ru --no-cache russian.wav              # routes to gigaam
wt --lang zh --no-cache mandarin.wav             # routes to whisper-cpp-large-v3-turbo-q8
wt --model whisper-cpp-large-v3-turbo-q8 --diarizer sortformer-onnx --speakers 3 audio_15s_3speakers.m4a
```

Expected: every clip produces correct `speakers_detected` and word-level timestamps. The whisper-cpp pass on the 4-speaker clip should now also produce one ASR segment per word, fixing the merge collapse.

### 5. Risk + cost notes

- whisper-rs first build pulls + compiles whisper.cpp via CMake. ~2 min on this machine without CUDA, ~5-8 min with CUDA. Subsequent builds are incremental.
- whisper.cpp CUDA build needs `CUDA_PATH` and `nvcc` on PATH. Confirm via `nvcc --version`.
- Silero LangID returns 95 codes; some (e.g. `nb`, `sr`) won't map to Parakeet or GigaAM — the router already falls through to whisper-cpp.
- The `ort` version pulled by `parakeet-rs` must match the one Silero LangID is exported for. If `Session::commit_from_file` errors with an opset mismatch, pin `ort = "2.0.0-rc.12"` (already transitively in tree) and re-export with `--opset 17`.
- Don't bundle the 874 MB whisper-cpp model in the installer — keep it download-on-demand like the other ASR models.
