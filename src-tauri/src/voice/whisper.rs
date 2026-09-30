use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperVadParams,
};

use super::wav;

/// A loaded model costs a second to open and hundreds of megabytes resident, so
/// it has to outlive the utterance that needed it. One slot rather than a map:
/// two models in memory at once buys nothing a user can tell apart.
struct Loaded {
    path: PathBuf,
    ctx: WhisperContext,
}

static ENGINE: OnceLock<Mutex<Option<Loaded>>> = OnceLock::new();

/// Set by the stop button. whisper-rs calls the abort hook before each GGML
/// computation and unwinds when it returns true, so this is a real cancel
/// rather than a discarded result.
static CANCEL: AtomicBool = AtomicBool::new(false);

fn slot() -> &'static Mutex<Option<Loaded>> {
    ENGINE.get_or_init(|| Mutex::new(None))
}

pub fn cancel() {
    CANCEL.store(true, Ordering::Relaxed);
}

/// Which model is resident right now, if any.
pub fn loaded_model() -> Option<PathBuf> {
    slot()
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(|l| l.path.clone()))
}

/// Close the model so the file underneath it can be replaced.
///
/// Windows will not rename over a file that is still open, so every path that
/// writes or deletes a model has to call this *before* touching the disk and
/// not after. Getting that order wrong fails at exactly the moment somebody
/// swaps models, which is the moment nobody is testing.
pub fn evict() {
    if let Ok(mut guard) = slot().lock() {
        *guard = None;
    }
}

fn load(path: &Path) -> Result<WhisperContext, String> {
    if !path.is_file() {
        return Err(format!(
            "the voice model is not downloaded yet — get {} from Settings → Voice",
            path.display()
        ));
    }

    let mut params = WhisperContextParameters::new();
    params.use_gpu(true);

    WhisperContext::new_with_params(path, params)
        .map_err(|err| format!("could not load the voice model: {err}"))
}

fn threads() -> i32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as i32)
        .unwrap_or(4)
}

/// Decode one clip into text.
///
/// Takes the samples rather than the encoded clip so the caller can decode
/// once and then run the cheap gates before paying for a whisper load.
///
/// `vad` is the silero model. It is the primary defence against the failure
/// this whole feature lives with: on silence, whisper does not return nothing,
/// it returns a confident sentence. VAD strips the silence before the decoder
/// ever sees it; the phrase filter in `super::filter` is only the backstop for
/// when the model is missing.
pub fn transcribe(model: &Path, vad: Option<&Path>, samples: &[i16]) -> Result<String, String> {
    if samples.is_empty() {
        return Err("there was no audio in that recording".to_string());
    }
    let pcm = wav::to_f32(samples);

    let mut guard = slot()
        .lock()
        .map_err(|_| "the voice engine stopped responding — restart Argus".to_string())?;

    if guard.as_ref().is_none_or(|l| l.path != model) {
        // Drop before loading: see `evict`. A new context for the same path
        // would still hold the old one open on Windows.
        *guard = None;
        *guard = Some(Loaded {
            path: model.to_path_buf(),
            ctx: load(model)?,
        });
    }

    let Some(loaded) = guard.as_ref() else {
        return Err("the voice model did not load".to_string());
    };

    CANCEL.store(false, Ordering::Relaxed);

    let mut state = loaded
        .ctx
        .create_state()
        .map_err(|err| format!("could not start a decode: {err}"))?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 5 });

    // set_language defaults to "en" and leaving it alone does NOT auto-detect.
    // The user chose auto-detect, so this has to be explicit or every
    // non-English dictation silently returns plausible English garbage.
    params.set_detect_language(true);
    params.set_translate(false);
    // The previous utterance is not context for this one.
    params.set_no_context(true);
    // whisper.cpp prints progress to stderr from C, on every decode.
    params.set_print_progress(false);
    params.set_print_realtime(false);
    // The default is min(4, cores), which leaves most of the machine idle.
    params.set_n_threads(threads());
    // Drops [MUSIC] and (upbeat music) before they become words.
    params.set_suppress_nst(true);
    params.set_abort_callback_safe(|| CANCEL.load(Ordering::Relaxed));

    if let Some(path) = vad {
        set_vad(&mut params, path)?;
    }

    state
        .full(params, &pcm)
        .map_err(|err| format!("the decode failed: {err}"))?;

    let mut out = String::new();
    for seg in state.as_iter() {
        if let Ok(text) = seg.to_str_lossy() {
            out.push_str(&text);
        }
    }

    Ok(out.trim().to_string())
}

/// Both of these panic in the bindings rather than returning an error:
/// `set_vad_model_path` on a NUL byte, and `enable_vad` when no path was set.
/// The path comes from our own allowlist today, but the panic is one refactor
/// away from being reachable, so it is checked here where it costs two lines.
fn set_vad(params: &mut FullParams<'_, '_>, path: &Path) -> Result<(), String> {
    let Some(raw) = path.to_str() else {
        return Err("the voice activity model path is not valid text".to_string());
    };

    if raw.contains('\0') {
        return Err("the voice activity model path is not valid".to_string());
    }

    if !path.is_file() {
        return Ok(());
    }

    let mut vad = WhisperVadParams::new();
    vad.set_threshold(0.5);
    vad.set_min_speech_duration(250);
    vad.set_min_silence_duration(100);
    vad.set_speech_pad(30);

    params.set_vad_model_path(Some(raw));
    params.set_vad_params(vad);
    params.enable_vad(true);

    Ok(())
}
