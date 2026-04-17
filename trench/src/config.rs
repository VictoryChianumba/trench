use std::collections::HashMap;
use std::path::PathBuf;

/// Ordered list of arXiv categories shown in the sources popup.
pub const KNOWN_ARXIV_CATS: &[(&str, &str)] = &[
  ("cs.LG", "machine learning"),
  ("cs.AI", "artificial intelligence"),
  ("cs.CL", "natural language processing"),
  ("cs.CV", "computer vision"),
  ("cs.NE", "neural networks"),
  ("cs.RO", "robotics"),
  ("stat.ML", "statistics / machine learning"),
];

/// Ordered list of predefined (non-custom) RSS sources.
pub const PREDEFINED_SOURCES: &[&str] =
  &["huggingface", "openai", "deepmind", "import_ai", "bair", "mit_news_ai"];

#[derive(Debug, serde::Serialize, serde::Deserialize, Default, Clone)]
pub struct Config {
  #[serde(default)]
  pub github_token: Option<String>,
  #[serde(default)]
  pub semantic_scholar_key: Option<String>,
  #[serde(default)]
  pub claude_api_key: Option<String>,
  #[serde(default)]
  pub openai_api_key: Option<String>,
  #[serde(default = "default_chat_provider")]
  pub default_chat_provider: String,
  #[serde(default)]
  pub sources: SourcesConfig,
}

fn default_chat_provider() -> String {
  "claude".to_string()
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct SourcesConfig {
  pub arxiv_categories: Vec<String>,
  pub enabled_sources: HashMap<String, bool>,
  pub custom_feeds: Vec<CustomFeed>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct CustomFeed {
  pub url: String,
  pub name: String,
  pub feed_type: String,
}

impl Default for SourcesConfig {
  fn default() -> Self {
    Self {
      arxiv_categories: vec![
        "cs.LG".to_string(),
        "cs.AI".to_string(),
        "stat.ML".to_string(),
      ],
      enabled_sources: HashMap::from([
        ("huggingface".to_string(), true),
        ("openai".to_string(), true),
        ("deepmind".to_string(), true),
        ("import_ai".to_string(), true),
        ("bair".to_string(), true),
        ("mit_news_ai".to_string(), true),
      ]),
      custom_feeds: vec![],
    }
  }
}

impl Config {
  pub fn load() -> Self {
    let path = match config_path() {
      Some(p) => p,
      None => return Config::default(),
    };
    let bytes = match std::fs::read(&path) {
      Ok(b) => b,
      Err(_) => return Config::default(),
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
  }

  pub fn save(&self) {
    let path = match config_path() {
      Some(p) => p,
      None => return,
    };
    if let Some(parent) = path.parent() {
      let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_vec_pretty(self) {
      let _ = std::fs::write(&path, &json);
      log::debug!(
        "config: wrote {} — contents:\n{}",
        path.display(),
        String::from_utf8_lossy(&json)
      );
    }
  }
}

fn config_path() -> Option<PathBuf> {
  let mut p = std::env::var_os("HOME").map(PathBuf::from)?;
  p.push(".config");
  p.push("trench");
  p.push("config.json");
  Some(p)
}
