#[derive(
  Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum SourcePlatform {
  ArXiv,
  Twitter,
  Blog,
  Newsletter,
  HuggingFace,
  Rss,
  OpenReview,
  Core,
}

impl SourcePlatform {
  pub fn short_label(&self) -> &'static str {
    match self {
      Self::ArXiv => "arXiv",
      Self::Twitter => "twit",
      Self::Blog => "blog",
      Self::Newsletter => "news",
      Self::HuggingFace => "hf",
      Self::Rss => "rss",
      Self::OpenReview => "OR",
      Self::Core => "CORE",
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkflowState {
  Inbox,
  Queued,
  DeepRead,
  Archived,
}

impl WorkflowState {
  pub fn short_label(&self) -> &'static str {
    match self {
      Self::Inbox => "inbox",
      Self::Queued => "queued",
      Self::DeepRead => "read",
      Self::Archived => "archived",
    }
  }
}

// Manual Deserialize that maps unknown values (legacy variants like
// `"skimmed"`, or anything else) to `Inbox`. This prevents a single removed
// or renamed enum variant from failing to deserialize the whole state file
// and silently zeroing user workflow tags via `unwrap_or_default()`.
impl<'de> serde::Deserialize<'de> for WorkflowState {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    let s = String::deserialize(deserializer)?;
    Ok(match s.as_str() {
      "inbox" => Self::Inbox,
      "queued" => Self::Queued,
      "deepread" => Self::DeepRead,
      "archived" => Self::Archived,
      _ => Self::Inbox,
    })
  }
}

#[cfg(test)]
mod workflow_state_tests {
  use super::WorkflowState;

  #[test]
  fn deserializes_known_variants() {
    let cases = [
      ("\"inbox\"", WorkflowState::Inbox),
      ("\"queued\"", WorkflowState::Queued),
      ("\"deepread\"", WorkflowState::DeepRead),
      ("\"archived\"", WorkflowState::Archived),
    ];
    for (json, expected) in cases {
      let got: WorkflowState = serde_json::from_str(json).unwrap();
      assert_eq!(got, expected, "input: {json}");
    }
  }

  #[test]
  fn deserializes_legacy_skimmed_to_inbox() {
    let got: WorkflowState = serde_json::from_str("\"skimmed\"").unwrap();
    assert_eq!(got, WorkflowState::Inbox);
  }

  #[test]
  fn deserializes_unknown_to_inbox() {
    for s in ["\"garbage\"", "\"\"", "\"INBOX\"", "\"future_variant\""] {
      let got: WorkflowState = serde_json::from_str(s).unwrap();
      assert_eq!(got, WorkflowState::Inbox, "input: {s}");
    }
  }

  #[test]
  fn serializes_to_lowercase_variant_name() {
    assert_eq!(serde_json::to_string(&WorkflowState::Inbox).unwrap(), "\"inbox\"");
    assert_eq!(serde_json::to_string(&WorkflowState::Queued).unwrap(), "\"queued\"");
    assert_eq!(serde_json::to_string(&WorkflowState::DeepRead).unwrap(), "\"deepread\"");
    assert_eq!(serde_json::to_string(&WorkflowState::Archived).unwrap(), "\"archived\"");
  }

  #[test]
  fn round_trips_via_json() {
    for v in [
      WorkflowState::Inbox,
      WorkflowState::Queued,
      WorkflowState::DeepRead,
      WorkflowState::Archived,
    ] {
      let s = serde_json::to_string(&v).unwrap();
      let back: WorkflowState = serde_json::from_str(&s).unwrap();
      assert_eq!(v, back);
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
      | SourcePlatform::Twitter
      | SourcePlatform::OpenReview => SignalLevel::Secondary,
      SourcePlatform::Newsletter
      | SourcePlatform::Rss
      | SourcePlatform::Core => SignalLevel::Tertiary,
    }
  }
}
