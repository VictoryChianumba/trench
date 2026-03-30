use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{ChatMessage, Role};
use crate::provider::ChatProvider;

pub struct ClaudeProvider {
    pub api_key: String,
    pub model: String,
}

impl ClaudeProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: "claude-sonnet-4-20250514".to_string(),
        }
    }

    pub fn with_model(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self { api_key: api_key.into(), model: model.into() }
    }
}

#[derive(Deserialize)]
struct ContentBlock {
    text: String,
}

#[derive(Deserialize)]
struct ClaudeResponse {
    content: Vec<ContentBlock>,
}

impl ChatProvider for ClaudeProvider {
    fn name(&self) -> &str {
        "claude"
    }

    fn send(&self, messages: &[ChatMessage]) -> Result<String> {
        // Collect system messages into a single system prompt; exclude from messages array.
        let system_text: String = messages
            .iter()
            .filter(|m| matches!(m.role, Role::System))
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");

        let api_messages: Vec<Value> = messages
            .iter()
            .filter(|m| !matches!(m.role, Role::System))
            .map(|m| {
                let role = match m.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::System => unreachable!(),
                };
                json!({ "role": role, "content": m.content })
            })
            .collect();

        let mut body = json!({
            "model": self.model,
            "max_tokens": 4096,
            "messages": api_messages,
        });

        if !system_text.is_empty() {
            body["system"] = json!(system_text);
        }

        let client = reqwest::blocking::Client::new();
        let resp = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()?;

        let status = resp.status();
        let text = resp.text()?;

        if !status.is_success() {
            return Err(anyhow!("Claude API error {status}: {text}"));
        }

        let parsed: ClaudeResponse = serde_json::from_str(&text)
            .map_err(|e| anyhow!("Failed to parse Claude response: {e}\nBody: {text}"))?;

        parsed
            .content
            .into_iter()
            .next()
            .map(|b| b.text)
            .ok_or_else(|| anyhow!("Claude response had no content blocks"))
    }
}
