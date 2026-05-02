use serde::Deserialize;

use crate::models::{
  ContentType, FeedItem, SignalLevel, SourcePlatform, WorkflowState,
  detect_subtopics,
};

pub fn fetch() -> Result<Vec<FeedItem>, String> {
  let mut items = Vec::new();
  for page in 1..=2u32 {
    let url = format!(
      "https://paperswithcode.com/api/v1/papers/?format=json&page={page}"
    );
    let resp = crate::http::client()
      .get(&url)
      .send()
      .map_err(|e| format!("papers_with_code: HTTP failed: {e}"))?;
    let body = crate::http::read_body(resp)
      .map_err(|e| format!("papers_with_code: read body failed: {e}"))?;
    let page_items = parse_page(&body)?;
    let done = page_items.is_empty();
    items.extend(page_items);
    if done {
      break;
    }
    std::thread::sleep(std::time::Duration::from_millis(500));
  }
  Ok(items)
}

fn parse_page(body: &str) -> Result<Vec<FeedItem>, String> {
  let resp: PwcResponse = serde_json::from_str(body)
    .map_err(|e| format!("papers_with_code: JSON parse failed: {e}"))?;

  let mut items = Vec::new();
  for paper in resp.results {
    let arxiv_id = match paper.arxiv_id.filter(|s| !s.is_empty()) {
      Some(id) => id,
      None => continue,
    };

    let canonical_url = format!("https://arxiv.org/abs/{arxiv_id}");
    let title = super::collapse_whitespace(&paper.title);
    let summary =
      super::collapse_whitespace(&paper.paper_abstract.unwrap_or_default());
    let authors: Vec<String> = paper
      .authors
      .into_iter()
      .map(|a| a.full_name)
      .filter(|n| !n.is_empty())
      .collect();
    let published_at = paper
      .published
      .and_then(|d| d.split('T').next().map(|s| s.to_string()))
      .unwrap_or_default();
    let domain_tags: Vec<String> =
      detect_subtopics(&title, &summary).iter().map(|s| s.to_string()).collect();

    // Prefer official repo; fall back to the repo with the most stars.
    let github_repo = paper
      .repositories
      .iter()
      .find(|r| r.is_official.unwrap_or(false))
      .or_else(|| {
        paper.repositories.iter().max_by_key(|r| r.stars.unwrap_or(0))
      })
      .map(|r| r.url.clone());

    let mut item = FeedItem {
      id: canonical_url.clone(),
      title,
      source_platform: SourcePlatform::PapersWithCode,
      content_type: ContentType::Paper,
      domain_tags,
      signal: SignalLevel::Secondary,
      published_at,
      authors,
      summary_short: summary,
      workflow_state: WorkflowState::Inbox,
      url: canonical_url,
      upvote_count: 0,
      github_repo,
      github_owner: None,
      github_repo_name: None,
      benchmark_results: vec![],
      full_content: None,
      source_name: "papers_with_code".to_string(),
    };
    item.signal = item.compute_signal();
    items.push(item);
  }
  Ok(items)
}

#[derive(Deserialize)]
struct PwcResponse {
  results: Vec<PwcPaper>,
}

#[derive(Deserialize)]
struct PwcPaper {
  arxiv_id: Option<String>,
  title: String,
  #[serde(rename = "abstract")]
  paper_abstract: Option<String>,
  #[serde(default)]
  authors: Vec<PwcAuthor>,
  published: Option<String>,
  #[serde(default)]
  repositories: Vec<PwcRepo>,
}

#[derive(Deserialize)]
struct PwcAuthor {
  full_name: String,
}

#[derive(Deserialize)]
struct PwcRepo {
  url: String,
  #[serde(default)]
  is_official: Option<bool>,
  #[serde(default)]
  stars: Option<u32>,
}
