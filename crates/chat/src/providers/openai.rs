use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{ChatMessage, Role};
use crate::provider::ChatProvider;

pub struct OpenAiProvider {
    pub api_key: String,
    pub model: String,
}

impl OpenAiProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: "gpt-4o".to_string(),
        }
    }

    pub fn with_model(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self { api_key: api_key.into(), model: model.into() }
    }
}

#[derive(Deserialize)]
struct Message {
    content: String,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct OpenAiResponse {
    choices: Vec<Choice>,
}

impl ChatProvider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }

    fn send(&self, messages: &[ChatMessage]) -> Result<String> {
        let api_messages: Vec<Value> = messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                };
                json!({ "role": role, "content": m.content })
            })
            .collect();

        let body = json!({
            "model": self.model,
            "messages": api_messages,
        });

        let client = reqwest::blocking::Client::new();
        let resp = client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json")
            .json(&body)
            .send()?;

        let status = resp.status();
        let text = resp.text()?;

        if !status.is_success() {
            return Err(anyhow!("OpenAI API error {status}: {text}"));
        }

        let parsed: OpenAiResponse = serde_json::from_str(&text)
            .map_err(|e| anyhow!("Failed to parse OpenAI response: {e}\nBody: {text}"))?;

        parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| anyhow!("OpenAI response had no choices"))
    }
}
