pub mod client;
pub mod discord;
pub mod exa;
pub mod figma;
pub mod github;
pub mod gitlab;
pub mod google;
pub mod ha;
pub mod linear;
pub mod notion;
pub mod outlook;
pub mod slack;
pub mod spotify;
pub mod telegram;
pub mod todoist;
pub mod trello;
pub mod vault;
use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};

use serde_json::Value;
use tokio::sync::Mutex as AsyncMutex;

use crate::connectors::{store, Connector};

pub const PROTOCOL_VERSION: &str = "2025-06-18";

pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("argus-agent")
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

#[derive(Clone, Debug)]
pub struct ToolInfo {
    pub name: String,
    pub desc: String,
    pub schema: Value,
}

struct Entry {
    cfg: Connector,
    client: Arc<AsyncMutex<Option<client::Client>>>,
}

static REGISTRY: OnceLock<StdMutex<HashMap<String, Entry>>> = OnceLock::new();

fn registry() -> &'static StdMutex<HashMap<String, Entry>> {
    REGISTRY.get_or_init(StdMutex::default)
}

pub fn configure(cfgs: Vec<Connector>) {
    let mut reg = registry().lock().unwrap_or_else(|p| p.into_inner());

    *reg = cfgs
        .into_iter()
        .map(|cfg| {
            (
                cfg.id.clone(),
                Entry {
                    cfg,
                    client: Arc::new(AsyncMutex::new(None)),
                },
            )
        })
        .collect();
}

pub fn sync_from(conn: &rusqlite::Connection) -> Result<usize, String> {
    let list = store::enabled(conn)?;
    let n = list.len();
    configure(list);
    Ok(n)
}

async fn session(
    server: &str,
) -> Result<(Arc<AsyncMutex<Option<client::Client>>>, Connector), String> {
    let entry = {
        let reg = registry().lock().unwrap_or_else(|p| p.into_inner());
        reg.get(server)
            .map(|err| (err.client.clone(), err.cfg.clone()))
            .ok_or_else(|| format!("unknown connector {server}"))?
    };

    Ok(entry)
}

pub async fn tools(server: &str) -> Result<Vec<ToolInfo>, String> {
    let (client, cfg) = session(server).await?;
    // Take the client out of its slot so the call never runs under the registry
    // guard. A wedged server must not serialize every other caller.
    let mut cli = {
        let mut g = client.lock().await;
        let taken = g.take();
        drop(g);
        match taken {
            Some(mut c) => {
                if c.alive() {
                    c
                } else {
                    client::Client::spawn(&cfg).await?
                }
            }
            None => client::Client::spawn(&cfg).await?,
        }
    };

    let out = cli.tools().await;
    restore(&client, cli).await;
    out
}

pub async fn call(server: &str, tool: &str, args: &Value) -> Result<String, String> {
    let (client, cfg) = session(server).await?;
    let mut cli = {
        let mut g = client.lock().await;
        let taken = g.take();
        drop(g);
        match taken {
            Some(mut c) => {
                if c.alive() {
                    c
                } else {
                    client::Client::spawn(&cfg).await?
                }
            }
            None => client::Client::spawn(&cfg).await?,
        }
    };

    let err = match cli.call(tool, args).await {
        Ok(out) => {
            restore(&client, cli).await;
            return Ok(out);
        }
        Err(err) => err,
    };

    if !err.starts_with("mcp ") {
        restore(&client, cli).await;
        return Err(err);
    }

    let tail = cli.tail();
    drop(cli);

    let mut cli = client::Client::spawn(&cfg).await.map_err(|err| {
        if tail.is_empty() {
            err
        } else {
            format!("{err}\n{tail}")
        }
    })?;

    let out = cli.call(tool, args).await;
    restore(&client, cli).await;
    out
}

/// Store a client back only if the slot is free and it is still alive: a dead
/// client would be retried as a corpse.
async fn restore(slot: &Arc<AsyncMutex<Option<client::Client>>>, mut cli: client::Client) {
    if !cli.alive() {
        return;
    }

    let mut g = slot.lock().await;

    if g.is_none() {
        *g = Some(cli);
    }
}
