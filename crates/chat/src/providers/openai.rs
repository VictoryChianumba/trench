use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{ChatMessage, Role};
use crate::provider::{ChatProvider, ProviderResponse};

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

#[derive(Deserialize, Default)]
struct Usage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
}

#[derive(Deserialize)]
struct OpenAiResponse {
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<Usage>,
}

impl ChatProvider for OpenAiProvider {
    fn name(&self) -> &str { "openai" }
    fn model(&self) -> &str { &self.model }
    fn context_window(&self) -> u64 { 128_000 }

    fn send(&self, messages: &[ChatMessage]) -> Result<ProviderResponse> {
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
            let friendly = friendly_error(status.as_u16(), &text);
            return Err(anyhow!("{friendly}"));
        }

        let parsed: OpenAiResponse = serde_json::from_str(&text)
            .map_err(|e| anyhow!("Failed to parse OpenAI response: {e}\nBody: {text}"))?;

        let content = parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| anyhow!("OpenAI response had no choices"))?;

        let (input_tokens, output_tokens) = parsed
            .usage
            .map(|u| (u.prompt_tokens, u.completion_tokens))
            .unwrap_or((0, 0));

        Ok(ProviderResponse { content, input_tokens, output_tokens })
    }
}

fn friendly_error(status: u16, body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(body) {
        if let Some(err_type) = v["error"]["type"].as_str() {
            return match err_type {
                "invalid_api_key" | "invalid_request_error" if status == 401 => {
                    "invalid API key — check settings".to_string()
                }
                t if t.contains("rate_limit") => "rate limit exceeded — try again shortly".to_string(),
                t if t.contains("quota") || t.contains("billing") => {
                    "quota exceeded — check billing".to_string()
                }
                _ => {
                    let msg = v["error"]["message"].as_str().unwrap_or("unknown error");
                    let short = if msg.len() > 80 { &msg[..80] } else { msg };
                    format!("API error — {short}")
                }
            };
        }
    }
    format!("API error {status}")
}
