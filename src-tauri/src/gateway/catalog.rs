use super::schema::{Avail, ModelEntry, Provider};

// Costs are per 1k tokens (in, out). Free = 0.
pub fn providers() -> Vec<Provider> {
  vec![
    Provider {
      id: "ollama".into(),
      compatible: "openAI".into(),
      base_url: "http://localhost:11434/v1".into(),
      api_key_ref: None,
      enabled: true,
      free: true,
      priority: 10,
    },
    Provider {
      id: "openrouter".into(),
      compatible: "openAI".into(),
      base_url: "https://openrouter.ai/api/v1".into(),
      api_key_ref: Some("argus-gw/openrouter".into()),
      enabled: true,
      free: false,
      priority: 50,
    },
    Provider {
      id: "groq".into(),
      compatible: "openAI".into(),
      base_url: "https://api.groq.com/openai/v1".into(),
      api_key_ref: Some("argus-gw/groq".into()),
      enabled: true,
      free: false,
      priority: 60,
    },
    Provider {
      id: "baseten".into(),
      compatible: "openAI".into(),
      base_url: "https://inference.baseten.co/v1".into(),
      api_key_ref: Some("argus-gw/baseten".into()),
      enabled: true,
      free: false,
      priority: 70,
    },
    Provider {
      id: "anthropic".into(),
      compatible: "Anthropic".into(),
      base_url: "https://api.anthropic.com".into(),
      api_key_ref: Some("argus-gw/anthropic".into()),
      enabled: true,
      free: false,
      priority: 40,
    },
  ]
}

pub fn models() -> Vec<ModelEntry> {
  vec![
    ModelEntry {
      id: "deepseek-v3.1".into(),
      display_name: "DeepSeek V3.1".into(),
      family: Some("deepseek".into()),
      capabilities: Some(r#"{"tools":true,"reasoning":true,"context":163840}"#.into()),
      suggested_tier: Some("mid".into()),
    },
    ModelEntry {
      id: "llama-3.3-70b".into(),
      display_name: "Llama 3.3 70B".into(),
      family: Some("llama".into()),
      capabilities: Some(r#"{"tools":true,"context":131072}"#.into()),
      suggested_tier: Some("cheap".into()),
    },
    ModelEntry {
      id: "gpt-oss-120b".into(),
      display_name: "GPT-OSS 120B".into(),
      family: Some("gpt-oss".into()),
      capabilities: Some(r#"{"tools":true,"reasoning":true,"context":131072}"#.into()),
      suggested_tier: Some("cheap".into()),
    },
    ModelEntry {
      id: "qwen3-coder".into(),
      display_name: "Qwen3 Coder".into(),
      family: Some("qwen".into()),
      capabilities: Some(r#"{"tools":true,"context":262144}"#.into()),
      suggested_tier: Some("mid".into()),
    },
    ModelEntry {
      id: "claude-sonnet-5".into(),
      display_name: "Claude Sonnet 5".into(),
      family: Some("claude".into()),
      capabilities: Some(r#"{"tools":true,"vision":true,"reasoning":true,"context":200000}"#.into()),
      suggested_tier: Some("frontier".into()),
    },
  ]
}

pub fn avail() -> Vec<Avail> {
  vec![
    Avail { model_id: "deepseek-v3.1".into(), provider_id: "openrouter".into(), remote_model_id: "deepseek/deepseek-chat-v3.1".into(), cost_in: 0.00027, cost_out: 0.0011 },
    Avail { model_id: "deepseek-v3.1".into(), provider_id: "ollama".into(), remote_model_id: "deepseek-v3.1".into(), cost_in: 0.0, cost_out: 0.0 },
    Avail { model_id: "llama-3.3-70b".into(), provider_id: "groq".into(), remote_model_id: "llama-3.3-70b-versatile".into(), cost_in: 0.00059, cost_out: 0.00079 },
    Avail { model_id: "llama-3.3-70b".into(), provider_id: "ollama".into(), remote_model_id: "llama3.3:70b".into(), cost_in: 0.0, cost_out: 0.0 },
    Avail { model_id: "gpt-oss-120b".into(), provider_id: "groq".into(), remote_model_id: "openai/gpt-oss-120b".into(), cost_in: 0.00015, cost_out: 0.00075 },
    Avail { model_id: "gpt-oss-120b".into(), provider_id: "ollama".into(), remote_model_id: "gpt-oss:120b".into(), cost_in: 0.0, cost_out: 0.0 },
    Avail { model_id: "qwen3-coder".into(), provider_id: "openrouter".into(), remote_model_id: "qwen/qwen3-coder".into(), cost_in: 0.0003, cost_out: 0.0012 },
    Avail { model_id: "qwen3-coder".into(), provider_id: "ollama".into(), remote_model_id: "qwen3-coder".into(), cost_in: 0.0, cost_out: 0.0 },
    Avail { model_id: "claude-sonnet-5".into(), provider_id: "anthropic".into(), remote_model_id: "claude-sonnet-5".into(), cost_in: 0.003, cost_out: 0.015 },
  ]
}
