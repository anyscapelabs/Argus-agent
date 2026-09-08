use serde_json::Value;

use super::{clip, expand};

pub fn read(args: &Value) -> Result<String, String> {
  let p = args["path"].as_str().ok_or("fs.read needs a path")?;
  let p = expand(p);
  let txt = std::fs::read_to_string(&p).map_err(|e| format!("{p}: {e}"))?;
  Ok(clip(txt))
}

pub fn list(args: &Value) -> Result<String, String> {
  let p = args["path"].as_str().unwrap_or(".");
  let p = expand(p);
  let mut out: Vec<String> = vec![];
  for e in std::fs::read_dir(&p).map_err(|e| format!("{p}: {e}"))? {
    let e = e.map_err(|e| e.to_string())?;
    let name = e.file_name().to_string_lossy().into_owned();
    if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
      out.push(format!("{name}/"));
    } else {
      out.push(name);
    }
  }
  out.sort();
  if out.is_empty() {
    return Ok("(empty)".into());
  }
  Ok(clip(out.join("\n")))
}

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
