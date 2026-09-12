pub mod client;

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};

use serde_json::Value;
use tokio::sync::Mutex as AsyncMutex;

use crate::connectors::{store, Connector};

pub const PROTOCOL_VERSION: &str = "2025-06-18";

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
    let mut g = client.lock().await;

    if !g.as_mut().map(|c| c.alive()).unwrap_or(false) {
        *g = None;
    }

    if g.is_none() {
        *g = Some(client::Client::spawn(&cfg).await?);
    }

    let Some(cli) = g.as_mut() else {
        return Err("mcp gone".into());
    };

    match cli.tools().await {
        Ok(t) => Ok(t),
        Err(err) if err.starts_with("mcp ") => {
            *g = None;
            Err(err)
        }
        Err(err) => Err(err),
    }
}

pub async fn call(server: &str, tool: &str, args: &Value) -> Result<String, String> {
    let (client, cfg) = session(server).await?;
    let mut g = client.lock().await;

    if !g.as_mut().map(|c| c.alive()).unwrap_or(false) {
        *g = None;
    }

    if g.is_none() {
        *g = Some(client::Client::spawn(&cfg).await?);
    }

    let Some(cli) = g.as_mut() else {
        return Err("mcp gone".into());
    };

    let err = match cli.call(tool, args).await {
        Ok(out) => return Ok(out),
        Err(err) => err,
    };

    if !err.starts_with("mcp ") {
        return Err(err);
    }

    let Some(cur) = g.as_ref() else {
        return Err(err);
    };

    let tail = cur.tail();
    *g = None;

    *g = Some(client::Client::spawn(&cfg).await.map_err(|err| {
        if tail.is_empty() {
            err
        } else {
            format!("{err}\n{tail}")
        }
    })?);

    let Some(cli) = g.as_mut() else {
        return Err("mcp gone".into());
    };

    cli.call(tool, args).await
}
