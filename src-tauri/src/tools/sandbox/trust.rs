use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Trust {
    Trusted,
    Untrusted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    UserFile,
    ProjectDir,
    Paste,
    WebFetch(String),
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
