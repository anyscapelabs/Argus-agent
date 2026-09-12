use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::sync::{oneshot, Mutex as AsyncMutex};

use super::{ToolInfo, PROTOCOL_VERSION};
use crate::connectors::Connector;

const REQ_TIMEOUT: Duration = Duration::from_secs(60);
const ERR_TAIL: usize = 800;

pub struct Client {
    _child: Child,
    stdin: AsyncMutex<ChildStdin>,
    next_id: StdMutex<u64>,
    pending: Arc<StdMutex<HashMap<u64, oneshot::Sender<McpRes>>>>,
    err_tail: Arc<StdMutex<String>>,
}

type McpRes = Result<serde_json::Value, String>;

const ENV_ALLOW: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "LANG",
    "LC_ALL",
    "TMPDIR",
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "XDG_RUNTIME_DIR",
    "XDG_DATA_DIRS",
    "XDG_CONFIG_HOME",
    "XDG_CACHE_HOME",
];

impl Client {
    pub async fn spawn(cfg: &Connector) -> Result<Self, String> {
        let mut cmd = tokio::process::Command::new(&cfg.command);
        cmd.args(&cfg.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .env_clear();

        for (k, v) in std::env::vars() {
            if ENV_ALLOW.contains(&k.as_str()) {
                cmd.env(k, v);
            }
        }

        for (k, v) in &cfg.env {
            cmd.env(k, v);
        }

        let mut child = cmd
            .spawn()
            .map_err(|err| format!("{}: {err}", cfg.command))?;

        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let stderr = child.stderr.take().ok_or("no stderr")?;

        let pending: Arc<StdMutex<HashMap<u64, oneshot::Sender<McpRes>>>> =
            Arc::new(StdMutex::new(HashMap::new()));
        let err_tail = Arc::new(StdMutex::new(String::new()));

        let p2 = pending.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();

            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
                            continue;
                        };

                        let Some(id) = v.get("id").and_then(|i| i.as_u64()) else {
                            continue;
                        };

                        if let Some(tx) = p2.lock().unwrap_or_else(|p| p.into_inner()).remove(&id) {
                            let res = match v.get("error") {
                                Some(err) => Err(format!(
                                    "mcp error {}: {}",
                                    err.get("code").and_then(|c| c.as_i64()).unwrap_or(0),
                                    err.get("message").and_then(|m| m.as_str()).unwrap_or("?")
                                )),
                                None => Ok(v.get("result").cloned().unwrap_or_default()),
                            };

                            let _ = tx.send(res);
                        }
                    }
                    _ => {
                        let mut g = p2.lock().unwrap_or_else(|p| p.into_inner());
                        for (_, tx) in g.drain() {
                            let _ = tx.send(Err("mcp server exited".into()));
                        }
                        break;
                    }
                }
            }
        });

        let t2 = err_tail.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();

            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        let mut g = t2.lock().unwrap_or_else(|p| p.into_inner());
                        g.push_str(&line);
                        g.push('\n');
                        let len = g.len();

                        if len > ERR_TAIL {
                            let cut = len - ERR_TAIL;
                            *g = g.split_off(cut);
                        }
                    }
                    _ => break,
                }
            }
        });

        let mut c = Client {
            _child: child,
            stdin: AsyncMutex::new(stdin),
            next_id: StdMutex::new(0),
            pending,
            err_tail,
        };

        c.request(
            "initialize",
            &serde_json::json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "argus", "version": env!("CARGO_PKG_VERSION")},
            }),
        )
        .await?;

        c.notify("notifications/initialized").await?;

        Ok(c)
    }

    async fn request(&mut self, method: &str, params: &serde_json::Value) -> McpRes {
        let id = {
            let mut g = self.next_id.lock().unwrap_or_else(|p| p.into_inner());
            *g += 1;
            *g
        };

        let msg = serde_json::json!({
            "jsonrpc": "2.0", "id": id, "method": method, "params": params,
        });

        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(id, tx);

        {
            let mut g = self.stdin.lock().await;
            if g.write_all(format!("{msg}\n").as_bytes()).await.is_err() {
                self.pending
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .remove(&id);
                return Err("mcp server pipe is closed".into());
            }
        }

        match tokio::time::timeout(REQ_TIMEOUT, rx).await {
            Ok(Ok(res)) => res,
            Ok(Err(_)) => Err("mcp server dropped the response".into()),
            Err(_) => {
                self.pending
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .remove(&id);
                Err("mcp request timed out after 60s".into())
            }
        }
    }

    async fn notify(&mut self, method: &str) -> Result<(), String> {
        let msg = serde_json::json!({"jsonrpc": "2.0", "method": method});

        let mut g = self.stdin.lock().await;
        g.write_all(format!("{msg}\n").as_bytes())
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn tools(&mut self) -> Result<Vec<ToolInfo>, String> {
        let res = self.request("tools/list", &serde_json::json!({})).await?;

        let Some(list) = res.get("tools").and_then(|t| t.as_array()) else {
            return Err("tools/list returned no tools array".into());
        };

        Ok(list
            .iter()
            .filter_map(|t| {
                Some(ToolInfo {
                    name: t.get("name")?.as_str()?.to_string(),
                    desc: t
                        .get("description")
                        .and_then(|d| d.as_str())
                        .unwrap_or("")
                        .to_string(),
                    schema: t.get("inputSchema").cloned().unwrap_or_default(),
                })
            })
            .collect())
    }

    pub async fn call(&mut self, tool: &str, args: &serde_json::Value) -> Result<String, String> {
        let res = self
            .request(
                "tools/call",
                &serde_json::json!({"name": tool, "arguments": args}),
            )
            .await?;

        let mut out = String::new();

        if let Some(parts) = res.get("content").and_then(|c| c.as_array()) {
            for p in parts {
                if p.get("type").and_then(|t| t.as_str()) == Some("text") {
                    if let Some(t) = p.get("text").and_then(|t| t.as_str()) {
                        if !out.is_empty() {
                            out.push('\n');
                        }
                        out.push_str(t);
                    }
                }
            }
        }

        if res.get("isError").and_then(|err| err.as_bool()) == Some(true) {
            return Err(if out.is_empty() {
                "tool failed".into()
            } else {
                out
            });
        }

        Ok(out)
    }

    pub fn alive(&mut self) -> bool {
        matches!(self._child.try_wait(), Ok(None))
    }

    pub fn tail(&self) -> String {
        self.err_tail
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .trim()
            .to_string()
    }
}
