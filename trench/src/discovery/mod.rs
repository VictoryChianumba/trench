pub mod ai_query;
pub mod pipeline;

use crate::models::FeedItem;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiscoveryPlan {
  #[serde(default)]
  pub topic: String,
  #[serde(default)]
  pub arxiv_categories: Vec<String>,
  #[serde(default)]
  pub paper_ids: Vec<String>,
  #[serde(default)]
  pub rss_urls: Vec<DiscoveredRssFeed>,
  #[serde(default)]
  pub github_sources: Vec<DiscoveredSource>,
  #[serde(default)]
  pub huggingface_sources: Vec<DiscoveredSource>,
  #[serde(default)]
  pub search_terms: Vec<String>,
  #[serde(default)]
  pub summary: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiscoveredRssFeed {
  pub url: String,
  pub name: String,
  #[serde(default)]
  pub reason: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiscoveredSource {
  pub url: String,
  #[serde(default)]
  pub kind: String,
  #[serde(default)]
  pub reason: String,
}

pub enum DiscoveryMessage {
  PlanReady(DiscoveryPlan),
  Items(Vec<FeedItem>),
  Complete,
  Error(String),
}
