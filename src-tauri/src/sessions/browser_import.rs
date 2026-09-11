use std::fs;
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Emitter, Manager};

use crate::tools::browser;

const SKIP_DIRS: &[&str] = &[
    "Cache",
    "Code Cache",
    "GPUCache",
    "GrShaderCache",
    "ShaderCache",
    "Service Worker",
    "OptimizationGuidePredictionModels",
    "Crashpad",
    "component_crx_cache",
    "extensions_crx_cache",
];

const PROGRESS_STEP: u64 = 32 * 1_048_576;

fn source_profile() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;

    for name in [
        "google-chrome",
        "google-chrome-beta",
        "chromium",
        "BraveSoftware/Brave-Browser",
    ] {
        let p = Path::new(&home).join(".config").join(name);
        if p.join("Local State").is_file() {
            return Some(p);
        }
    }

    None
}

fn copy_tree(
    src: &Path,
    dst: &Path,
    app: &AppHandle,
    copied: &mut u64,
    next: &mut u64,
) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("{}: {e}", dst.display()))?;

    for entry in fs::read_dir(src).map_err(|e| format!("{}: {e}", src.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name = name.to_string_lossy().into_owned();
        let from = entry.path();
        let to = dst.join(&name);

        if from.is_dir() {
            if SKIP_DIRS.contains(&name.as_str())
                || name.starts_with("Singleton")
                || name.ends_with(".tmp")
            {
                continue;
            }

            copy_tree(&from, &to, app, copied, next)?;
        } else {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            fs::copy(&from, &to).map_err(|e| format!("{}: {e}", from.display()))?;

            *copied += size;
            if *copied >= *next {
                *next = *copied + PROGRESS_STEP;
                let _ = app.emit("browser-import-progress", *copied);
            }
        }
    }

    Ok(())
}

#[tauri::command]
pub fn sess_browser_import(app: AppHandle, profile: String) -> Result<(), String> {
    let dst = browser::profile_dir(&profile);
    let src = source_profile().ok_or("no Chrome/Chromium profile found in ~/.config")?;

    browser::close_profile(&profile);

    if dst.exists() {
        fs::remove_dir_all(&dst).map_err(|e| format!("clearing old profile failed: {e}"))?;
    }

    fs::create_dir_all(&dst).map_err(|e| e.to_string())?;

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let dir = app.path().app_data_dir().ok();
        if let Some(d) = dir {
            let _ = fs::create_dir_all(d.join("browser-profiles"));
        }

        let mut copied = 0u64;
        let mut next = PROGRESS_STEP;
        let _ = app.emit("browser-import-progress", 0u64);

        let res = copy_tree(&src, &dst, &app, &mut copied, &mut next)
            .map(|_| format!("imported {:.0} MB", copied as f64 / 1_048_576.0));

        match res {
            Ok(note) => {
                let _ = app.emit("browser-import-done", Ok::<String, String>(note));
            }
            Err(e) => {
                let _ = fs::remove_dir_all(&dst);
                let _ = app.emit(
                    "browser-import-done",
                    Err::<String, String>(format!("import failed: {e}")),
                );
            }
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_cache_dirs_wherever_they_live() {
        assert!(SKIP_DIRS.contains(&"Cache"));
        assert!(SKIP_DIRS.contains(&"Service Worker"));
        assert!(SKIP_DIRS.contains(&"component_crx_cache"));
    }
}
