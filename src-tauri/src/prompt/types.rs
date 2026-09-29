// What assembly returns: the projection the loop consumes and the read-only
// preview the UI shows.
use serde::Serialize;

use crate::gateway::schema::{ChatReq, WireMsg};

pub struct Projection {
    pub child: bool,
    pub session_id: String,
    pub model_id: Option<String>,
    pub system: String,
    pub summary: Option<String>,
    pub msgs: Vec<WireMsg>,
    pub prefix_hash: String,
    pub ctx_tokens: i64,
    pub compact_seq: i64,
    pub web: bool,
}

impl Projection {
    pub fn chat_req(&self) -> ChatReq {
        let mut msgs = vec![WireMsg {
            role: "system".into(),
            content: self.system.clone(),
            ..Default::default()
        }];
        msgs.extend(self.msgs.iter().cloned());

        // A sub-agent cannot fan out. Taking the tool away is what enforces
        // depth one; the prompt only asks nicely.
        let specs: Vec<_> = crate::tools::tool_specs(self.web)
            .into_iter()
            .filter(|t| !(self.child && t.name.starts_with("agent.")))
            .collect();

        ChatReq {
            model: self.model_id.clone().unwrap_or_default(),
            msgs,
            prefix_hash: None,
            tools: specs,
        }
    }
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PromptPreview {
    pub session_id: String,
    pub model_id: Option<String>,
    pub system: String,
    pub summary: Option<String>,
    pub msgs: Vec<WireMsg>,
    pub prefix_hash: String,
    pub ctx_tokens: i64,
    pub compact_seq: i64,
}
