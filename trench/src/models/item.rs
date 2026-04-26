#[derive(
  Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum SourcePlatform {
  ArXiv,
  Twitter,
  Blog,
  PapersWithCode,
  Newsletter,
  HuggingFace,
  Rss,
}

impl SourcePlatform {
  pub fn short_label(&self) -> &'static str {
    match self {
      Self::ArXiv => "arXiv",
      Self::Twitter => "twit",
      Self::Blog => "blog",
      Self::PapersWithCode => "pwc",
      Self::Newsletter => "news",
      Self::HuggingFace => "hf",
      Self::Rss => "rss",
    }
  }
}

#[derive(
  Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum ContentType {
  Paper,
  Thread,
  Article,
  Repo,
  Digest,
}

impl ContentType {
  pub fn short_label(&self) -> &'static str {
    match self {
      Self::Paper => "paper",
      Self::Thread => "thread",
      Self::Article => "article",
      Self::Repo => "repo",
      Self::Digest => "digest",
    }
  }
}

#[derive(
  Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum SignalLevel {
  Primary,
  Secondary,
  Tertiary,
}

impl SignalLevel {
  pub fn indicator(&self) -> &'static str {
    match self {
      Self::Primary => "*",
      Self::Secondary => "·",
      Self::Tertiary => " ",
    }
  }
}

#[derive(
  Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum WorkflowState {
  Inbox,
  Skimmed,
  Queued,
  DeepRead,
  Archived,
}

impl WorkflowState {
  pub fn short_label(&self) -> &'static str {
    match self {
      Self::Inbox => "inbox",
      Self::Skimmed => "skimmed",
      Self::Queued => "queued",
      Self::DeepRead => "read",
      Self::Archived => "archived",
    }
  }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BenchmarkResult {
  pub task: String,
  pub dataset: String,
  pub metric: String,
  pub score: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FeedItem {
  pub id: String,
  pub title: String,
  pub source_platform: SourcePlatform,
  pub content_type: ContentType,
  pub domain_tags: Vec<String>,
  pub signal: SignalLevel,
  pub published_at: String,
  pub authors: Vec<String>,
  pub summary_short: String,
  pub workflow_state: WorkflowState,
  pub url: String,
  #[serde(default)]
  pub upvote_count: u32,
  #[serde(default)]
  pub github_repo: Option<String>,
  #[serde(default)]
  pub github_owner: Option<String>,
  #[serde(default)]
  pub github_repo_name: Option<String>,
  #[serde(default)]
  pub benchmark_results: Vec<BenchmarkResult>,
  #[serde(default)]
  pub full_content: Option<String>,
  #[serde(default)]
  pub source_name: String,
}

impl FeedItem {
  /// Derive signal level from source and upvotes.
  pub fn compute_signal(&self) -> SignalLevel {
    match self.source_platform {
      SourcePlatform::ArXiv => SignalLevel::Primary,
      SourcePlatform::HuggingFace => {
        if self.upvote_count >= 10 {
          SignalLevel::Primary
        } else if self.upvote_count >= 1 {
          SignalLevel::Secondary
        } else {
          SignalLevel::Tertiary
        }
      }
      SourcePlatform::Blog
      | SourcePlatform::PapersWithCode
      | SourcePlatform::Twitter => SignalLevel::Secondary,
      SourcePlatform::Newsletter | SourcePlatform::Rss => SignalLevel::Tertiary,
    }
  }
}
