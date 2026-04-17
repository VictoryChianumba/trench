use crate::config::Config;
use crate::discovery::{DiscoveredRssFeed, DiscoveredSource, DiscoveryPlan};
use serde::Deserialize;
use serde_json::{Value, json};

const CLAUDE_MODEL: &str = "claude-sonnet-4-20250514";
const OPENAI_MODEL: &str = "gpt-4o";

pub fn run_ai_query(
  topic: &str,
  config: &Config,
) -> Result<DiscoveryPlan, String> {
  let prompt = build_prompt(topic);
  let content = if let Some(key) =
    config.claude_api_key.as_ref().filter(|k| !k.trim().is_empty())
  {
    query_claude(key, &prompt)?
  } else if let Some(key) =
    config.openai_api_key.as_ref().filter(|k| !k.trim().is_empty())
  {
    query_openai(key, &prompt)?
  } else {
    return Err("No Claude or OpenAI API key configured".to_string());
  };

  parse_plan(topic, &content)
}

fn build_prompt(topic: &str) -> String {
  format!(
    r#"You are a research discovery assistant for an AI/ML feed reader.
Topic: "{topic}"

Return ONLY valid JSON. Do not include markdown, code fences, or explanation.

Required JSON shape:
{{
  "topic": "{topic}",
  "arxiv_categories": ["cs.LG"],
  "paper_ids": ["2312.01234v1"],
  "rss_urls": [{{"url": "https://example.com/feed", "name": "Example", "reason": "why relevant"}}],
  "github_sources": [{{"url": "https://github.com/org/repo", "kind": "repo", "reason": "why relevant"}}],
  "huggingface_sources": [{{"url": "https://huggingface.co/papers", "kind": "search", "reason": "why relevant"}}],
  "search_terms": ["sparse autoencoder mechanistic interpretability"],
  "summary": "One sentence summary."
}}

Rules:
- arxiv_categories: at most 8 relevant category codes.
- paper_ids: at most 20 specific arXiv IDs you are confident exist.
- rss_urls: at most 10 RSS/blog feed URLs directly relevant to this topic.
- github_sources: at most 10 GitHub org/repo URLs; checklist only.
- huggingface_sources: at most 10 HuggingFace URLs; checklist only.
- search_terms: 1 to 5 specific phrases for arXiv search.
- summary: 80 characters or fewer."#
  )
}

fn query_claude(api_key: &str, prompt: &str) -> Result<String, String> {
  #[derive(Deserialize)]
  struct ClaudeBlock {
    text: String,
  }
  #[derive(Deserialize)]
  struct ClaudeResponse {
    content: Vec<ClaudeBlock>,
  }

  let body = json!({
    "model": CLAUDE_MODEL,
    "max_tokens": 1024,
    "messages": [{ "role": "user", "content": prompt }],
  });

  let client = reqwest::blocking::Client::new();
  let resp = client
    .post("https://api.anthropic.com/v1/messages")
    .header("x-api-key", api_key)
    .header("anthropic-version", "2023-06-01")
    .header("content-type", "application/json")
    .json(&body)
    .send()
    .map_err(|e| format!("Claude request failed: {e}"))?;

  let status = resp.status();
  let text =
    resp.text().map_err(|e| format!("Claude body read failed: {e}"))?;
  if !status.is_success() {
    return Err(format!("Claude API error {}: {}", status.as_u16(), text));
  }

  let parsed: ClaudeResponse = serde_json::from_str(&text)
    .map_err(|e| format!("Claude response parse failed: {e}"))?;
  parsed
    .content
    .into_iter()
    .next()
    .map(|b| b.text)
    .ok_or_else(|| "Claude response had no content".to_string())
}

fn query_openai(api_key: &str, prompt: &str) -> Result<String, String> {
  #[derive(Deserialize)]
  struct OpenAiMessage {
    content: String,
  }
  #[derive(Deserialize)]
  struct OpenAiChoice {
    message: OpenAiMessage,
  }
  #[derive(Deserialize)]
  struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
  }

  let body = json!({
    "model": OPENAI_MODEL,
    "messages": [{ "role": "user", "content": prompt }],
  });

  let client = reqwest::blocking::Client::new();
  let resp = client
    .post("https://api.openai.com/v1/chat/completions")
    .header("Authorization", format!("Bearer {api_key}"))
    .header("content-type", "application/json")
    .json(&body)
    .send()
    .map_err(|e| format!("OpenAI request failed: {e}"))?;

  let status = resp.status();
  let text =
    resp.text().map_err(|e| format!("OpenAI body read failed: {e}"))?;
  if !status.is_success() {
    return Err(format!("OpenAI API error {}: {}", status.as_u16(), text));
  }

  let parsed: OpenAiResponse = serde_json::from_str(&text)
    .map_err(|e| format!("OpenAI response parse failed: {e}"))?;
  parsed
    .choices
    .into_iter()
    .next()
    .map(|c| c.message.content)
    .ok_or_else(|| "OpenAI response had no choices".to_string())
}

fn parse_plan(topic: &str, content: &str) -> Result<DiscoveryPlan, String> {
  let stripped = strip_json_fence(content);
  let raw: RawDiscoveryPlan = serde_json::from_str(stripped)
    .map_err(|e| format!("Discovery JSON parse failed: {e}"))?;

  let mut plan = DiscoveryPlan {
    topic: if raw.topic.trim().is_empty() {
      topic.to_string()
    } else {
      raw.topic.trim().to_string()
    },
    arxiv_categories: raw
      .arxiv_categories
      .into_iter()
      .filter_map(|cat| normalize_arxiv_category(&cat))
      .take(8)
      .collect(),
    paper_ids: raw
      .paper_ids
      .into_iter()
      .filter_map(|id| normalize_arxiv_id(&id))
      .take(20)
      .collect(),
    rss_urls: raw
      .rss_urls
      .into_iter()
      .filter_map(normalize_rss_feed)
      .take(10)
      .collect(),
    github_sources: raw
      .github_sources
      .into_iter()
      .filter_map(normalize_source)
      .take(10)
      .collect(),
    huggingface_sources: raw
      .huggingface_sources
      .into_iter()
      .filter_map(normalize_source)
      .take(10)
      .collect(),
    search_terms: raw
      .search_terms
      .into_iter()
      .map(|s| s.trim().to_string())
      .filter(|s| !s.is_empty())
      .take(5)
      .collect(),
    summary: raw.summary.chars().take(80).collect(),
  };

  plan.arxiv_categories.sort();
  plan.arxiv_categories.dedup();
  plan.paper_ids.sort();
  plan.paper_ids.dedup();
  plan.search_terms.sort();
  plan.search_terms.dedup();

  Ok(plan)
}

#[derive(Deserialize)]
struct RawDiscoveryPlan {
  #[serde(default)]
  topic: String,
  #[serde(default)]
  arxiv_categories: Vec<String>,
  #[serde(default, alias = "arxiv_ids")]
  paper_ids: Vec<String>,
  #[serde(default)]
  rss_urls: Vec<Value>,
  #[serde(default)]
  github_sources: Vec<Value>,
  #[serde(default, alias = "huggingface")]
  huggingface_sources: Vec<Value>,
  #[serde(default)]
  search_terms: Vec<String>,
  #[serde(default)]
  summary: String,
}

fn strip_json_fence(content: &str) -> &str {
  let trimmed = content.trim();
  if !trimmed.starts_with("```") {
    return trimmed;
  }

  let without_open = trimmed
    .strip_prefix("```json")
    .or_else(|| trimmed.strip_prefix("```"))
    .unwrap_or(trimmed)
    .trim_start();
  without_open.strip_suffix("```").unwrap_or(without_open).trim()
}

fn normalize_arxiv_category(value: &str) -> Option<String> {
  let cat = value.trim();
  if cat.len() > 20 || !cat.contains('.') {
    return None;
  }
  let valid =
    cat.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
  if valid { Some(cat.to_string()) } else { None }
}

pub fn normalize_arxiv_id(value: &str) -> Option<String> {
  let mut id = value.trim();
  for prefix in
    &["https://arxiv.org/abs/", "http://arxiv.org/abs/", "arxiv.org/abs/"]
  {
    if let Some(rest) = id.strip_prefix(prefix) {
      id = rest;
      break;
    }
  }
  id = id.split(['?', '#']).next().unwrap_or(id);
  if let Some(v_pos) = id.rfind('v') {
    let version = &id[v_pos + 1..];
    if !version.is_empty() && version.chars().all(|c| c.is_ascii_digit()) {
      id = &id[..v_pos];
    }
  }
  let valid = id.contains('.')
    && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
  if valid { Some(id.to_string()) } else { None }
}

fn normalize_rss_feed(value: Value) -> Option<DiscoveredRssFeed> {
  match value {
    Value::String(url) => {
      if !is_http_url(&url) {
        return None;
      }
      Some(DiscoveredRssFeed {
        name: feed_name_from_url(&url),
        url,
        reason: String::new(),
      })
    }
    Value::Object(mut obj) => {
      let url = obj.remove("url")?.as_str()?.trim().to_string();
      if !is_http_url(&url) {
        return None;
      }
      let name = obj
        .remove("name")
        .or_else(|| obj.remove("title"))
        .and_then(|v| v.as_str().map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| feed_name_from_url(&url));
      let reason = obj
        .remove("reason")
        .and_then(|v| v.as_str().map(|s| s.trim().to_string()))
        .unwrap_or_default();
      Some(DiscoveredRssFeed { url, name, reason })
    }
    _ => None,
  }
}

fn normalize_source(value: Value) -> Option<DiscoveredSource> {
  match value {
    Value::String(url) => {
      if !is_http_url(&url) {
        return None;
      }
      Some(DiscoveredSource { url, kind: String::new(), reason: String::new() })
    }
    Value::Object(mut obj) => {
      let url = obj
        .remove("url")
        .or_else(|| obj.remove("value"))?
        .as_str()?
        .trim()
        .to_string();
      if !is_http_url(&url) {
        return None;
      }
      let kind = obj
        .remove("kind")
        .and_then(|v| v.as_str().map(|s| s.trim().to_string()))
        .unwrap_or_default();
      let reason = obj
        .remove("reason")
        .and_then(|v| v.as_str().map(|s| s.trim().to_string()))
        .unwrap_or_default();
      Some(DiscoveredSource { url, kind, reason })
    }
    _ => None,
  }
}

pub fn is_http_url(url: &str) -> bool {
  url.starts_with("http://") || url.starts_with("https://")
}

fn feed_name_from_url(url: &str) -> String {
  url
    .trim_start_matches("https://")
    .trim_start_matches("http://")
    .split('/')
    .next()
    .unwrap_or("feed")
    .to_string()
}
