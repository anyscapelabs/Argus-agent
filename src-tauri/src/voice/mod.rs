pub mod download;
pub mod filter;
pub mod wav;
pub mod whisper;

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::Manager;

use crate::gateway::{store, Gateway};

use download::Event;

const KV_MODEL: &str = "voice.stt_model";
const KV_LANG: &str = "voice.language";

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub stt_model: String,
    pub language: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            stt_model: "base-q5_1".to_string(),
            language: "auto".to_string(),
        }
    }
}

#[derive(Serialize)]
pub struct ModelRow {
    #[serde(flatten)]
    pub info: &'static download::ModelInfo,
    pub installed: bool,
    pub active: bool,
    pub bundled: bool,
}

#[derive(Serialize)]
pub struct Status {
    pub ready: bool,
    pub model: String,
    pub installed: bool,
    pub bundled: bool,
    pub vad_installed: bool,
    pub engine_loaded: bool,
}

/// Read the clip and decide whether it is worth keeping.
///
/// The two filter halves sit either side of the decode on purpose: `precheck`
/// refuses a silent or runaway clip for free, and only something that passed
/// is worth a model load. Errors here are the user-facing sentences from
/// `verdict_msg`, so the frontend has nothing to phrase.
pub fn run(model: &Path, vad: Option<&Path>, wav_b64: &str) -> Result<String, String> {
    let samples = wav::decode_b64(wav_b64)?;

    if let Some(v) = filter::precheck(&samples) {
        return Err(filter::verdict_msg(v).to_string());
    }

    let text = whisper::transcribe(model, vad, &samples)?;

    match filter::postcheck(&text) {
        Some(v) => Err(filter::verdict_msg(v).to_string()),
        None => Ok(text),
    }
}

pub fn config_from(conn: &Connection) -> Result<Config, String> {
    let fallback = Config::default();

    Ok(Config {
        stt_model: store::kv_get(conn, KV_MODEL).unwrap_or(fallback.stt_model),
        language: store::kv_get(conn, KV_LANG).unwrap_or(fallback.language),
    })
}

pub fn set_config(conn: &Connection, cfg: &Config) -> Result<(), String> {
    if download::model_for(&cfg.stt_model).is_none() {
        return Err(format!("there is no voice model called {}", cfg.stt_model));
    }

    store::kv_set(conn, KV_MODEL, &cfg.stt_model)?;
    store::kv_set(conn, KV_LANG, &cfg.language)
}

pub fn rows(dir: &Path, bundled_dir: Option<&Path>, cfg: &Config) -> Vec<ModelRow> {
    download::MODELS
        .iter()
        .map(|info| {
            let bundled = bundled_dir.is_some_and(|b| b.join(info.file).is_file());

            ModelRow {
                info,
                installed: bundled || dir.join(info.file).is_file(),
                active: info.key == cfg.stt_model,
                bundled,
            }
        })
        .collect()
}

fn app_data(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(|err| err.to_string())
}

/// The copy that shipped inside the app.
///
/// Clicking the mic has to start recording. Every model the user did not ask
/// for is fetched in Settings, but the one that came with the installer is
/// already on disk, and a build that forgot it is the only case that reaches
/// for a download.
fn bundled_dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    let dir = app.path().resource_dir().ok()?.join("voice");

    dir.is_dir().then_some(dir)
}

/// Prefer the bundled model; fall back to the user's directory.
fn resolve(app: &tauri::AppHandle, key: &str) -> Result<(PathBuf, Option<PathBuf>), String> {
    let dir = app.path().app_data_dir().map_err(|err| err.to_string())?;
    let user_dir = download::models_dir(&dir);

    let bundled = bundled_dir(app);
    let model = bundled
        .as_ref()
        .and_then(|b| download::model_for(key).map(|i| b.join(i.file)))
        .filter(|p| p.is_file())
        .or_else(|| {
            let p = download::model_path(&user_dir, key).ok()?;
            p.is_file().then_some(p)
        })
        .ok_or_else(|| {
            format!(
                "The {} voice model is missing. Reinstall Argus, or pick another in Settings.",
                download::model_for(key).map_or(key.as_str(), |i| i.label)
            )
        })?;

    let vad = bundled
        .as_ref()
        .map(|b| b.join(download::VAD_FILE))
        .filter(|p| p.is_file())
        .or_else(|| {
            let p = download::vad_path(&user_dir);
            p.is_file().then_some(p)
        });

    Ok((model, vad))
}

#[tauri::command]
pub fn voice_config(gw: tauri::State<'_, Gateway>) -> Result<Config, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;

    config_from(&conn)
}

#[tauri::command]
pub fn voice_set_config(gw: tauri::State<'_, Gateway>, cfg: Config) -> Result<(), String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;

    set_config(&conn, &cfg)
}

#[tauri::command]
pub fn voice_models(app: tauri::AppHandle, gw: tauri::State<'_, Gateway>) -> Result<Vec<ModelRow>, String> {
    let dir = download::models_dir(&app_data(&app)?);
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;

    Ok(rows(&dir, bundled_dir(&app).as_deref(), &config_from(&conn)?))
}

#[tauri::command]
pub fn voice_status(app: tauri::AppHandle, gw: tauri::State<'_, Gateway>) -> Result<Status, String> {
    let dir = download::models_dir(&app_data(&app)?);
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    let cfg = config_from(&conn)?;

    let bundled = bundled_dir(&app);
    let is_bundled = bundled.as_ref().and_then(|b| {
        download::model_for(&cfg.stt_model).map(|i| b.join(i.file).is_file())
    });

    Ok(Status {
        ready: is_bundled.unwrap_or_else(|| download::has(&dir, &cfg.stt_model)),
        model: cfg.stt_model,
        installed: is_bundled.unwrap_or_else(|| download::has(&dir, &cfg.stt_model)),
        bundled: is_bundled.unwrap_or(false),
        vad_installed: bundled
            .as_ref()
            .is_some_and(|b| b.join(download::VAD_FILE).is_file())
            || download::vad_path(&dir).is_file(),
        engine_loaded: whisper::loaded_model().is_some(),
    })
}

/// Fetch one model, plus the voice activity model.
///
/// Both come from the same request because the VAD model is 864 KB against 57
/// MB — invisible in the progress bar — and because a dictation box that
/// silently hallucinates on silence is worse than one that is slow to install.
/// It is the primary defence; the phrase filter is only the backstop.
#[tauri::command]
pub async fn voice_download_model(
    app: tauri::AppHandle,
    gw: tauri::State<'_, Gateway>,
    key: String,
    on_event: Channel<Event>,
) -> Result<(), String> {
    let dir = download::models_dir(&app_data(&app)?);

    let info = download::model_for(&key)
        .ok_or_else(|| format!("there is no voice model called {key}"))?;

    // Already on disk inside the app. Re-fetching it would spend 57 MB to
    // replace a file the installer owns and the user cannot edit.
    if bundled_dir(&app).is_some_and(|b| b.join(info.file).is_file()) {
        return Err(format!("{} already ships with Argus.", info.label));
    }

    let client = crate::gateway::http_client().map_err(|err| err.to_string())?;
    let mut failures = Vec::new();

    // The engine may hold this very file open. Windows cannot rename onto an
    // open file, so evict before the first byte is written, not after.
    whisper::evict();

    let jobs = [
        (download::WHISPER_REPO, info.file, dir.join(info.file), key.clone()),
        (
            download::VAD_REPO,
            download::VAD_FILE,
            download::vad_path(&dir),
            format!("{key}:vad"),
        ),
    ];

    for (repo, file, dest, label) in jobs {
        if dest.is_file() {
            continue;
        }

        let tx = on_event.clone();
        let result = download::fetch(&client, repo, file, &dest, |got, total| {
            let _ = tx.send(Event::Progress {
                key: label.clone(),
                got,
                total,
            });
        })
        .await;

        if let Err(msg) = result {
            failures.push(msg.clone());
            let _ = on_event.send(Event::Failed {
                key: label,
                msg: msg.clone(),
            });
        } else {
            let _ = on_event.send(Event::Done { key: label });
        }
    }

    if !failures.is_empty() {
        return Err(failures.join("; "));
    }

    let conn = gw.conn.lock().map_err(|err| err.to_string())?;

    set_config(&conn, &Config {
        stt_model: key,
        language: config_from(&conn)?.language,
    })
}

#[tauri::command]
pub fn voice_delete_model(app: tauri::AppHandle, key: String) -> Result<(), String> {
    let info = download::model_for(&key)
        .ok_or_else(|| format!("there is no voice model called {key}"))?;

    // It came with the app. It is also the only reason the mic works with no
    // network at all, so deleting it would trade a convenience for nothing.
    if bundled_dir(&app).is_some_and(|b| b.join(info.file).is_file()) {
        return Err(format!("{} is the model Argus ships with, so it stays.", info.label));
    }

    let path = download::models_dir(&app_data(&app)?).join(info.file);

    if !path.is_file() {
        return Ok(());
    }

    // Same ordering as the download: the context has to let go of the file
    // before it is removed.
    if whisper::loaded_model().as_deref() == Some(path.as_path()) {
        whisper::evict();
    }

    std::fs::remove_file(&path).map_err(|err| format!("could not delete the model: {err}"))
}

#[tauri::command]
pub async fn voice_transcribe(
    app: tauri::AppHandle,
    gw: tauri::State<'_, Gateway>,
    wav_b64: String,
) -> Result<String, String> {
    let key = {
        // Scoped so the connection lock is gone before the await below. A
        // guard held across a suspension parks the database for every other
        // task in the app, and the decode takes seconds.
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        config_from(&conn)?.stt_model
    };

    let (model, vad) = resolve(&app, &key)?;

    tauri::async_runtime::spawn_blocking(move || run(&model, vad.as_deref(), &wav_b64))
        .await
        .map_err(|err| format!("the transcription thread stopped: {err}"))?
}

#[tauri::command]
pub fn voice_cancel() {
    whisper::cancel();
}