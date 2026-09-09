use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;

use crate::gateway::store as gw_store;

pub const DEFAULT_CONTEXT: i64 = 128_000;

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct CompressionCfg {
    pub enabled: bool,
    pub threshold: f64,
    pub target_ratio: f64,
    pub protect_first: i64,
    pub protect_last: i64,
    pub safety: f64,
    pub model_thresholds: HashMap<String, f64>,
}

impl Default for CompressionCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: 0.50,
            target_ratio: 0.20,
            protect_first: 3,
            protect_last: 20,
            safety: 0.85,
            model_thresholds: HashMap::new(),
        }
    }
}

impl CompressionCfg {
    pub fn load(conn: &Connection) -> Self {
        match gw_store::kv_get(conn, "compression") {
            Some(v) => serde_json::from_str(&v).unwrap_or_default(),
            None => Self::default(),
        }
    }

    pub fn threshold_for(&self, model_id: &str) -> f64 {
        let mut best: Option<(usize, f64)> = None;

        for (k, v) in &self.model_thresholds {
            if model_id.contains(k.as_str()) {
                let better = match best {
                    Some((len, _)) => k.len() > len,
                    None => true,
                };
                if better {
                    best = Some((k.len(), *v));
                }
            }
        }

        best.map(|(_, v)| v).unwrap_or(self.threshold)
    }
}

pub fn context_window(conn: &Connection, model_id: Option<&str>) -> i64 {
    let Some(m) = model_id else {
        return DEFAULT_CONTEXT;
    };

    let caps: Option<String> = conn
        .query_row(
            "SELECT capabilities FROM models WHERE id = ?1",
            params![m],
            |r| r.get(0),
        )
        .optional()
        .ok()
        .flatten();

    caps.and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
        .and_then(|v| v["context"].as_u64())
        .map(|c| c as i64)
        .filter(|c| *c > 0)
        .unwrap_or(DEFAULT_CONTEXT)
}

pub fn est_tokens(text: &str) -> i64 {
    text.len() as i64 / 4
}

pub fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }

    let cut: String = s.chars().take(max).collect();
    format!("{cut}…")
}
