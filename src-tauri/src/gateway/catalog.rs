use super::schema::Provider;

// Ollama seeds first run: local, keyless, no network. Every other provider and
// model comes from the models.dev sync.
pub fn providers() -> Vec<Provider> {
  vec![
    Provider {
      id: "ollama".into(),
      name: "Ollama".into(),
      compatible: "openAI".into(),
      base_url: "http://localhost:11434/v1".into(),
      api_key_ref: None,
      connected: false,
      free: true,
      priority: 10,
      logo_url: Some("https://models.dev/logos/ollama.svg".into()),
      doc_url: None,
    },
  ]
}
