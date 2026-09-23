use serde::Serialize;
use tauri::State;

use crate::gateway::Gateway;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Trust {
    Trusted,
    Untrusted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Files the user created or owns on this machine.
    UserFile,
    /// A project directory that already lived on disk before this task.
    ProjectDir,
    /// Content the user pasted into the chat themselves.
    Paste,
    /// Body text fetched from the web via web.read.
    WebFetch(String),
    /// A freshly cloned repository, host unknown to the user.
    GitClone(String),
}

const SHORTENERS: &[&str] = &[
    "bit.ly",
    "t.co",
    "tinyurl.com",
    "goo.gl",
    "is.gd",
    "cutt.ly",
];

fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let host = rest.split(['/', '?', ':']).next().unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or("");
    host.trim().to_lowercase()
}

fn is_raw_ip(host: &str) -> bool {
    host.parse::<std::net::IpAddr>().is_ok()
}

fn host_allowed(host: &str, allow_hosts: &[String]) -> bool {
    allow_hosts
        .iter()
        .any(|a| a == host || host.ends_with(&format!(".{a}")))
}

pub fn classify(origin: &Origin, allow_hosts: &[String]) -> Trust {
    match origin {
        Origin::UserFile | Origin::ProjectDir | Origin::Paste => Trust::Trusted,
        Origin::WebFetch(url) | Origin::GitClone(url) => {
            let host = host_of(url);

            if host.is_empty()
                || is_raw_ip(&host)
                || SHORTENERS.contains(&host.as_str())
                || !host_allowed(&host, allow_hosts)
            {
                return Trust::Untrusted;
            }

            Trust::Trusted
        }
    }
}

pub fn trust_of(origin: &Origin, allow_hosts: &[String]) -> Trust {
    classify(origin, allow_hosts)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustBadge {
    pub trust: Trust,
    pub origin: String,
}

pub fn badge(origin: &Origin, allow_hosts: &[String]) -> TrustBadge {
    TrustBadge {
        trust: classify(origin, allow_hosts),
        origin: String::from(match origin {
            Origin::UserFile => "user file",
            Origin::ProjectDir => "project directory",
            Origin::Paste => "pasted content",
            Origin::WebFetch(_) => "web fetch",
            Origin::GitClone(_) => "git clone",
        }),
    }
}

pub const KV_ALLOW_HOSTS: &str = "sandbox.allow_hosts";
pub const KV_ALLOW_IMAGES: &str = "sandbox.allow_images";
pub const KV_DEFAULT_IMAGE: &str = "sandbox.default_image";

pub fn detect_runtime() -> Option<&'static str> {
    for bin in ["podman", "docker"] {
        let ok = std::process::Command::new(bin)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if ok {
            return Some(bin);
        }
    }

    None
}

pub const INSTALL_HINT: &str =
    "no container runtime found — install podman (https://podman.io/getting-started/installation) to run untrusted code sandboxed";

pub const REFUSE_MSG: &str =
    "refusing to run untrusted code outside a container — ask the user to install podman or mark the source trusted";

fn image_allowed(image: &str, allow_images: &[String]) -> bool {
    allow_images.iter().any(|a| a == image)
}

fn container_args(
    runtime: &str,
    image: &str,
    workdir: &Path,
    command: &str,
    netted: bool,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "run".into(),
        "--rm".into(),
        "--read-only".into(),
        "--cap-drop=all".into(),
        "--security-opt=no-new-privileges".into(),
        "--pids-limit=256".into(),
        "--memory=2g".into(),
        "--cpus=2".into(),
    ];

    if netted {
        args.push("--network=bridge".into());
    } else {
        args.push("--network=none".into());
    }

    args.push(format!("-v={}:{}:rw", workdir.display(), "/work"));
    args.push("-w=/work".into());
    args.push(image.into());

    args.push(runtime.eq("docker").then_some("sh").unwrap_or("sh").into());
    args.push("-c".into());
    args.push(command.into());

    args
}

pub fn build_run(
    image: &str,
    workdir: &Path,
    command: &str,
    netted: bool,
    allow_images: &[String],
) -> Result<(String, Vec<String>), String> {
    let Some(runtime) = detect_runtime() else {
        return Err(INSTALL_HINT.into());
    };

    if !image_allowed(image, allow_images) {
        return Err(format!(
            "image '{image}' is not in the sandbox allowlist — add a digest-pinned image in Settings first"
        ));
    }

    Ok((
        runtime.into(),
        container_args(&runtime, image, workdir, command, netted),
    ))
}

use std::path::Path;

pub async fn run_sandboxed(
    gw: &crate::gateway::Gateway,
    workdir: &Path,
    command: &str,
    image: &str,
    netted: bool,
) -> Result<(String, i64), String> {
    let allow_images: Vec<String> = {
        let conn = gw.conn.lock().map_err(|err| err.to_string())?;
        crate::gateway::store::kv_get(&conn, KV_ALLOW_IMAGES)
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default()
    };

    let (runtime, cargs) = build_run(image, workdir, command, netted, &allow_images)?;
    let out = tokio::process::Command::new(&runtime)
        .args(&cargs)
        .current_dir(workdir)
        .output()
        .await
        .map_err(|err| err.to_string())?;

    let mut text = String::from_utf8_lossy(&out.stdout).to_string();

    if !out.stderr.is_empty() {
        text.push('\n');
        text.push_str(&String::from_utf8_lossy(&out.stderr));
    }

    let code = i64::from(out.status.code().unwrap_or(-1));

    Ok((text, code))
}

fn arg_str(args_json: &str, key: &str) -> String {
    serde_json::from_str::<serde_json::Value>(args_json)
        .ok()
        .and_then(|v| v.get(key).and_then(|f| f.as_str()).map(str::to_string))
        .unwrap_or_default()
}

fn first_url(text: &str) -> String {
    text.split_whitespace()
        .find(|t| {
            t.starts_with("https://")
                || t.starts_with("http://")
                || t.starts_with("git@")
                || t.ends_with(".git")
        })
        .unwrap_or("")
        .trim_matches(|c| c == '"' || c == '\'' || c == ',' || c == ')')
        .to_string()
}

pub fn origin_of_tool(tool: &str, args_json: &str) -> Option<Origin> {
    if tool.starts_with("web.") {
        return Some(Origin::WebFetch(arg_str(args_json, "url")));
    }

    if tool.starts_with("browser.") {
        return Some(Origin::WebFetch(arg_str(args_json, "url")));
    }

    if tool == "terminal" || tool == "bash.run" {
        let cmd = arg_str(args_json, "command");

        if cmd.split_whitespace().any(|w| w == "clone")
            && cmd.contains("git")
            && first_url(&cmd) != ""
        {
            return Some(Origin::GitClone(first_url(&cmd)));
        }
    }

    None
}

pub fn resolve_image(
    conn: &rusqlite::Connection,
    image_arg: Option<&str>,
) -> Result<String, String> {
    let allow_images: Vec<String> = crate::gateway::store::kv_get(conn, KV_ALLOW_IMAGES)
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default();

    if let Some(image) = image_arg.filter(|i| !i.trim().is_empty()) {
        if allow_images.iter().any(|a| a == image) {
            return Ok(image.to_string());
        }

        return Err(format!(
            "image '{image}' is not in the sandbox allowlist — pick one in Settings"
        ));
    }

    let def = crate::gateway::store::kv_get(conn, KV_DEFAULT_IMAGE).unwrap_or_default();

    if !def.trim().is_empty() && allow_images.iter().any(|a| a == &def) {
        return Ok(def);
    }

    Err("no sandbox image configured — add a digest-pinned image in Settings first".into())
}

pub fn validate_config(
    hosts: &[String],
    images: &[String],
    default_image: &str,
) -> Result<(), String> {
    for image in images {
        if !image.contains("@sha256:") {
            return Err(format!(
                "image '{image}' is not digest-pinned — use name:tag@sha256:<digest>"
            ));
        }
    }

    if !default_image.trim().is_empty() && !images.iter().any(|a| a == default_image) {
        return Err("default image must be one of the allowlisted images".into());
    }

    for host in hosts {
        if host.trim().is_empty() || host.contains(char::is_whitespace) {
            return Err(format!("bad host entry '{host}'"));
        }
    }

    Ok(())
}

#[derive(Serialize, serde::Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SandboxConfig {
    pub hosts: Vec<String>,
    pub images: Vec<String>,
    pub default_image: String,
}

fn read_config(conn: &rusqlite::Connection) -> SandboxConfig {
    let dec = |k: &str| -> Vec<String> {
        crate::gateway::store::kv_get(conn, k)
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or_default()
    };

    SandboxConfig {
        hosts: dec(KV_ALLOW_HOSTS),
        images: dec(KV_ALLOW_IMAGES),
        default_image: crate::gateway::store::kv_get(conn, KV_DEFAULT_IMAGE).unwrap_or_default(),
    }
}

#[tauri::command]
pub fn sandbox_config(gw: State<'_, Gateway>) -> Result<SandboxConfig, String> {
    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    Ok(read_config(&conn))
}

#[tauri::command]
pub fn sandbox_set_config(
    gw: State<'_, Gateway>,
    hosts: Vec<String>,
    images: Vec<String>,
    default_image: String,
) -> Result<SandboxConfig, String> {
    validate_config(&hosts, &images, &default_image)?;

    let conn = gw.conn.lock().map_err(|err| err.to_string())?;
    crate::gateway::store::kv_set(
        &conn,
        KV_ALLOW_HOSTS,
        &serde_json::to_string(&hosts).map_err(|err| err.to_string())?,
    )?;
    crate::gateway::store::kv_set(
        &conn,
        KV_ALLOW_IMAGES,
        &serde_json::to_string(&images).map_err(|err| err.to_string())?,
    )?;
    crate::gateway::store::kv_set(&conn, KV_DEFAULT_IMAGE, default_image.trim())?;

    Ok(read_config(&conn))
}

fn esc_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

pub fn origin_label(origin: Option<&Origin>, allow_hosts: &[String]) -> String {
    match origin {
        Some(o) => badge(o, allow_hosts).origin,
        None => "project".into(),
    }
}

pub fn record_block(command: &str, profile: &str, origin: &str, status: &str, out: &str) -> String {
    let body = out.replace('&', "&amp;").replace('<', "&lt;");

    format!(
        "<sandbox command=\"{}\" profile=\"{}\" origin=\"{}\" status=\"{}\">{}</sandbox>",
        esc_attr(command),
        esc_attr(profile),
        esc_attr(origin),
        esc_attr(status),
        body.trim()
    )
}
