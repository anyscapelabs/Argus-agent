use std::path::Path;

use serde_json::Value;

use super::expand;

pub fn write(args: &Value) -> Result<String, String> {
    let p = args["path"].as_str().ok_or("fs.write needs a path")?;
    let content = args["content"].as_str().unwrap_or("");

    let p = expand(p);

    if let Some(parent) = std::path::Path::new(&p).parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    std::fs::write(&p, content).map_err(|err| format!("{p}: {err}"))?;

    Ok(format!("wrote {p} ({} bytes)", content.len()))
}

pub fn remove_dir_fast(dir: &Path) -> Result<(), String> {
    if !dir.exists() {
        return Ok(());
    }

    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("bad dir path")?;
    let trash = dir.with_file_name(format!("{name}.trash-{}", std::process::id()));

    if std::fs::rename(dir, &trash).is_err() {
        return std::fs::remove_dir_all(dir).map_err(|err| err.to_string());
    }

    std::thread::spawn(move || {
        let _ = std::fs::remove_dir_all(&trash);
    });

    Ok(())
}
