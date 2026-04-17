use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub mod provider;
pub mod provider_registry;
pub mod providers;
pub mod storage;
pub mod ui;

pub use provider::{ChatProvider, ProviderResponse};
pub use provider_registry::{ProviderRegistry, parse_provider_prefix};
pub use providers::claude::ClaudeProvider;
pub use providers::openai::OpenAiProvider;
pub use storage::{
  create_session, delete_session, load_index, load_session, save_index,
  save_session,
};
pub use ui::{ChatAction, ChatSlashCommandSpec, ChatUi, ChatUiState};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Role {
  System,
  User,
  Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
  pub role: Role,
  pub content: String,
  pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
  pub id: String,
  pub title: String,
  pub created_at: DateTime<Utc>,
  pub updated_at: DateTime<Utc>,
  pub messages: Vec<ChatMessage>,
  pub provider: Option<String>,
  #[serde(default)]
  pub total_input_tokens: u64,
  #[serde(default)]
  pub total_output_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatIndex {
  pub sessions: Vec<ChatSessionMeta>,
  pub default_provider: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionMeta {
  pub id: String,
  pub title: String,
  pub created_at: DateTime<Utc>,
  pub updated_at: DateTime<Utc>,
  pub provider: Option<String>,
}
