use serde_json::Value;

use super::expand;

pub fn write(args: &Value) -> Result<String, String> {
  let p = args["path"].as_str().ok_or("fs.write needs a path")?;
  let content = args["content"].as_str().unwrap_or("");
  let p = expand(p);
  if let Some(parent) = std::path::Path::new(&p).parent() {
    let _ = std::fs::create_dir_all(parent);
  }
  std::fs::write(&p, content).map_err(|e| format!("{p}: {e}"))?;
  Ok(format!("wrote {p} ({} bytes)", content.len()))
}
