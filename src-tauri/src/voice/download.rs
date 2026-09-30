use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

pub const WHISPER_REPO: &str = "ggerganov/whisper.cpp";
pub const VAD_REPO: &str = "ggml-org/whisper-vad";
pub const VAD_FILE: &str = "ggml-silero-v6.2.0.bin";

/// A model the user may pick. The `file` field is the whole point: a name
/// arrives over IPC from JavaScript, and interpolating one into a URL and a
/// path is a traversal bug by construction. Nothing outside this list is ever
/// fetched, and the filename is written here, not by the caller.
#[derive(Clone, Copy, Serialize)]
pub struct ModelInfo {
    pub key: &'static str,
    pub file: &'static str,
    pub label: &'static str,
    pub mb: u64,
    pub note: &'static str,
}

pub const MODELS: &[ModelInfo] = &[
    ModelInfo {
        key: "base-q5_1",
        file: "ggml-base-q5_1.bin",
        label: "Base",
        mb: 57,
        note: "Fast and small. The default.",
    },
    ModelInfo {
        key: "small-q5_1",
        file: "ggml-small-q5_1.bin",
        label: "Small",
        mb: 181,
        note: "Noticeably better on names, code and paths, at three times the size.",
    },
    ModelInfo {
        key: "medium-q5_0",
        file: "ggml-medium-q5_0.bin",
        label: "Medium",
        mb: 539,
        note: "Best accuracy, but a long utterance can take twenty seconds.",
    },
    ModelInfo {
        key: "large-v3-q5_0",
        file: "ggml-large-v3-q5_0.bin",
        label: "Large v3",
        mb: 1_080,
        note: "Most accurate. Too slow to use while you are waiting to type.",
    },
];

/// The one way a caller turns a string from the frontend into a file.
pub fn model_for(key: &str) -> Option<&'static ModelInfo> {
    MODELS.iter().find(|m| m.key == key)
}

pub fn models_dir(app_data: &Path) -> PathBuf {
    app_data.join("voice-models")
}

pub fn model_path(dir: &Path, key: &str) -> Result<PathBuf, String> {
    let info = model_for(key).ok_or_else(|| format!("there is no voice model called {key}"))?;

    Ok(dir.join(info.file))
}

pub fn vad_path(dir: &Path) -> PathBuf {
    dir.join(VAD_FILE)
}

pub fn has(dir: &Path, key: &str) -> bool {
    match model_for(key) {
        Some(info) => dir.join(info.file).is_file(),
        None => false,
    }
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Progress { key: String, got: u64, total: u64 },
    Done { key: String },
    Failed { key: String, msg: String },
}

#[derive(Deserialize)]
struct TreeEntry {
    path: String,
    #[serde(default)]
    lfs: Option<LfsOid>,
}

#[derive(Deserialize)]
struct LfsOid {
    oid: String,
}

fn url_for(repo: &str, file: &str) -> String {
    format!("https://huggingface.co/{repo}/resolve/main/{file}")
}

/// The sha256 huggingface already stores for the file.
///
/// This catches a truncated or corrupted download, which is the realistic
/// failure. It does not catch a compromised origin, because the hash and the
/// bytes come from the same host — pinning the digests in this file would
/// close that, at the cost of a digest bump per model release.
async fn expected_sha(
    client: &reqwest::Client,
    repo: &str,
    file: &str,
) -> Result<String, String> {
    let url = format!("https://huggingface.co/api/models/{repo}/tree/main");

    let list: Vec<TreeEntry> = client
        .get(url)
        .send()
        .await
        .map_err(|err| format!("could not reach huggingface: {err}"))?
        .json()
        .await
        .map_err(|err| format!("could not read the model list from huggingface: {err}"))?;

    list.iter()
        .find(|entry| entry.path == file)
        .and_then(|entry| entry.lfs.as_ref().map(|lfs| lfs.oid.clone()))
        .ok_or_else(|| format!("huggingface did not publish a checksum for {file}"))
}

/// Stream one file to `<dest>.part`, verify it, then put it in place.
///
/// The rename is the last thing to happen, so an interrupted download leaves a
/// `.part` file and never a truncated model that looks complete. The caller is
/// responsible for having called `whisper::evict()` first: on Windows a rename
/// onto an open file fails.
pub async fn fetch(
    client: &reqwest::Client,
    repo: &str,
    file: &str,
    dest: &Path,
    on_progress: impl Fn(u64, u64),
) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let want = expected_sha(client, repo, file).await?;

    let resp = client
        .get(url_for(repo, file))
        .send()
        .await
        .map_err(|err| format!("could not reach huggingface: {err}"))?;

    if !resp.status().is_success() {
        return Err(format!(
            "huggingface returned {} for {file}",
            resp.status().as_u16()
        ));
    }

    let total = resp.content_length().unwrap_or(0);
    let mut stream = resp.bytes_stream();

    let part = dest.with_extension("part");
    let mut out = tokio::fs::File::create(&part).await.map_err(|err| err.to_string())?;

    let mut hasher = Sha256::new();
    let mut got: u64 = 0;
    let mut last_tick = Instant::now() - Duration::from_secs(1);

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| format!("the download stalled: {err}"))?;

        hasher.update(&chunk);
        out.write_all(&chunk).await.map_err(|err| err.to_string())?;
        got += chunk.len() as u64;

        // A channel message per chunk would be thousands of them for a 181 MB
        // model. Nothing the user can read changes faster than this.
        if last_tick.elapsed() >= Duration::from_millis(250) {
            on_progress(got, total);
            last_tick = Instant::now();
        }
    }

    out.flush().await.map_err(|err| err.to_string())?;
    drop(out);

    on_progress(got, total);

    let digest = hex(&hasher.finalize());
    if digest != want {
        let _ = tokio::fs::remove_file(&part).await;
        return Err("the downloaded model did not match its checksum — try again".to_string());
    }

    // Windows will not replace a file that is open, and the loaded context
    // holds one. Callers evict before getting here.
    if dest.exists() {
        let _ = tokio::fs::remove_file(dest).await;
    }

    tokio::fs::rename(&part, dest)
        .await
        .map_err(|err| format!("could not save the model: {err}"))?;

    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}