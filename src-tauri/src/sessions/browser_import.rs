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
];

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

fn copy_tree(src: &Path, dst: &Path, depth: usize) -> Result<u64, String> {
    let mut total = 0u64;

    for entry in fs::read_dir(src).map_err(|e| format!("{}: {e}", src.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name = name.to_string_lossy().into_owned();
        let from = entry.path();
        let to = dst.join(&name);

        if from.is_dir() {
            if depth == 0 && SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }

            if name.starts_with("Singleton") || name.ends_with(".tmp") {
                continue;
            }

            fs::create_dir_all(&to).map_err(|e| e.to_string())?;
            total += copy_tree(&from, &to, depth + 1)?;
        } else {
            if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
            fs::copy(&from, &to).map_err(|e| format!("{}: {e}", from.display()))?;
        }
    }

    Ok(total)
}

#[tauri::command]
pub fn sess_browser_import(app: AppHandle, profile: String) -> Result<(), String> {
    let dst = browser::profile_dir(&profile);
    let src = source_profile().ok_or("no Chrome/Chromium profile found in ~/.config")?;

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

        let res = copy_tree(&src, &dst, 0)
            .map(|bytes| format!("imported {:.0} MB", bytes as f64 / 1_048_576.0));

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
    fn skips_cache_dirs_at_top_level() {
        assert!(SKIP_DIRS.contains(&"Cache"));
        assert!(SKIP_DIRS.contains(&"Code Cache"));
        assert!(SKIP_DIRS.contains(&"Service Worker"));
    }

    #[test]
    fn keeps_nested_cache_like_dirs() {
        let name = "Cache".to_string();
        let depth = 1;

        let skip = depth == 0 && SKIP_DIRS.contains(&name.as_str());
        assert!(!skip);
    }
}
