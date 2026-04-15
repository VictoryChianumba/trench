use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub mod app;
pub mod colored_tags;
pub mod keymap;

pub use app::runner::{draw, handle_key};
pub mod editor;
pub mod entries_list;
pub mod filter;
pub mod history;
pub mod sorter;
pub mod storage;
pub mod theme;
pub mod ui;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
  pub article_id: String,
  pub article_title: String,
  pub article_url: String,
  pub content: String,
  pub tags: Vec<String>,
  pub created_at: DateTime<Utc>,
  pub updated_at: DateTime<Utc>,
}
