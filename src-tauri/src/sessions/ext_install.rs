use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};

use crate::tools::extpipe;

const HOST_NAME: &str = "com.argus.browser";
const HOST_DIRS: &[&str] = &[
    "google-chrome",
    "google-chrome-beta",
    "chromium",
    "BraveSoftware/Brave-Browser",
];

#[derive(Serialize)]
pub struct ExtInstall {
    ext_id: String,
    ext_path: String,
}

pub fn data_dir() -> PathBuf {
    let base = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_default();
            Path::new(&home).join(".local/share")
        });

    base.join("com.anyscapelabs.argus")
}

fn src_dir() -> PathBuf {
    let bundled = Path::new(env!("CARGO_MANIFEST_DIR")).join("../extension");

    if bundled.is_dir() {
        return bundled;
    }

    PathBuf::from("extension")
}

/// Chrome derives an unpacked extension's id from the sha256 of its absolute
/// path — hex digits mapped to a-p. Deterministic, so we can pin the host
/// manifest to it before the user ever loads the extension.
pub fn unpacked_id(dir: &Path) -> String {
    let hash = Sha256::digest(dir.to_string_lossy().as_bytes());
    let hex = format!("{:x}", hash);

    hex[..32]
        .chars()
        .map(|c| char::from(b'a' + c.to_digit(16).unwrap_or(0) as u8))
        .collect()
}

fn copy_tree(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("{}: {e}", dst.display()))?;

    for entry in fs::read_dir(src).map_err(|e| format!("{}: {e}", src.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let from = entry.path();
        let to = dst.join(entry.file_name());

        if from.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            fs::copy(&from, &to).map_err(|e| format!("{}: {e}", from.display()))?;
        }
    }

    Ok(())
}

fn write_wrapper(exe: &Path, path: &Path) -> Result<(), String> {
    let mut f = fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;

    writeln!(f, "#!/bin/sh").map_err(|e| e.to_string())?;
    writeln!(f, "exec \"{}\" --native-host", exe.display()).map_err(|e| e.to_string())?;

    fs::set_permissions(path, PermissionsExt::from_mode(0o755)).map_err(|e| e.to_string())
}

fn write_host_manifest(wrapper: &Path, ext_id: &str) -> Result<usize, String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let manifest = serde_json::json!({
        "name": HOST_NAME,
        "description": "Argus browser bridge",
        "path": wrapper.to_string_lossy(),
        "type": "stdio",
        "allowed_origins": [format!("chrome-extension://{ext_id}/")],
    });

    let mut done = 0;

    for dir in HOST_DIRS {
        let host_dir = Path::new(&home)
            .join(".config")
            .join(dir)
            .join("NativeMessagingHosts");

        if fs::create_dir_all(&host_dir).is_err() {
            continue;
        }

        let file = host_dir.join(format!("{HOST_NAME}.json"));
        let body = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;

        if fs::write(&file, body).is_ok() {
            done += 1;
        }
    }

    Ok(done)
}

fn chrome_running() -> bool {
    let out = std::process::Command::new("pgrep")
        .args(["-f", "(google-chrome|chromium)( |$)"])
        .output();

    matches!(out, Ok(o) if o.status.success())
}

fn chrome_bin() -> Option<&'static str> {
    for bin in ["google-chrome", "chromium"] {
        if std::process::Command::new(bin)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return Some(bin);
        }
    }

    None
}

/// Best-effort silent activation: works when Chrome isn't running and the
/// build still honours --load-extension. Branded Chrome 137+ ignores the
/// flag — the fallback is the guided chrome://extensions step.
fn launch_chrome(ext_dir: &Path) {
    if chrome_running() {
        return;
    }

    let Some(bin) = chrome_bin() else { return };

    let _ = std::process::Command::new(bin)
        .arg(format!("--load-extension={}", ext_dir.display()))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// All file work: copy extension, compute id, register host. Safe to re-run
/// on every startup — repairs the wrapper path after binary changes.
pub fn install_core(data: &Path) -> Result<ExtInstall, String> {
    let ext_dir = data.join("extension");

    if ext_dir.exists() {
        fs::remove_dir_all(&ext_dir).map_err(|e| e.to_string())?;
    }

    copy_tree(&src_dir(), &ext_dir)?;

    let ext_id = unpacked_id(&ext_dir);
    let wrapper = data.join("native-host.sh");
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;

    write_wrapper(&exe, &wrapper)?;
    write_host_manifest(&wrapper, &ext_id)?;

    Ok(ExtInstall {
        ext_id,
        ext_path: ext_dir.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub fn sess_ext_install(app: AppHandle) -> Result<ExtInstall, String> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app data dir: {e}"))?;

    let res = install_core(&data)?;

    let _ = fs::write(data.join("extension.enabled"), b"on");

    launch_chrome(&data.join("extension"));

    Ok(res)
}

#[tauri::command]
pub fn sess_ext_uninstall(app: AppHandle) -> Result<(), String> {
    let home = std::env::var("HOME").unwrap_or_default();

    for dir in HOST_DIRS {
        let file = Path::new(&home)
            .join(".config")
            .join(dir)
            .join("NativeMessagingHosts")
            .join(format!("{HOST_NAME}.json"));

        let _ = fs::remove_file(&file);
    }

    if let Ok(data) = app.path().app_data_dir() {
        let _ = fs::remove_file(data.join("extension.enabled"));
    }

    Ok(())
}

#[tauri::command]
pub fn sess_ext_status() -> bool {
    extpipe::connected()
}
