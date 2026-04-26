use crate::models::{
  ContentType, FeedItem, SignalLevel, SourcePlatform, WorkflowState,
  detect_subtopics,
};
use quick_xml::Reader;
use quick_xml::events::Event as XmlEvent;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

const HF_PAPERS_URL: &str = "https://huggingface.co/papers";

pub fn fetch() -> Result<Vec<FeedItem>, String> {
  let body = get_text(HF_PAPERS_URL)?;
  let today = today_date();
  let mut items = parse_papers(&body, &today);
  fetch_abstracts(&mut items);
  Ok(items)
}

// ---------------------------------------------------------------------------
// Abstract fetcher
// ---------------------------------------------------------------------------

/// Fetch abstracts from the arXiv API in a single batched request and fill in
/// `summary_short` on each item. Failures are logged and silently skipped.
fn fetch_abstracts(items: &mut Vec<FeedItem>) {
  if items.is_empty() {
    return;
  }

  let ids: Vec<&str> = items.iter().map(|i| i.id.as_str()).collect();
  let id_list = ids.join(",");
  let url = format!(
    "http://export.arxiv.org/api/query?id_list={id_list}&max_results=50"
  );

  log::info!(
    "huggingface: fetching abstracts for {} papers from arXiv",
    ids.len()
  );

  let body = match crate::http::client().get(&url).send().and_then(|r| {
    if r.status().is_success() { Ok(r) } else { Err(r.error_for_status().unwrap_err()) }
  }) {
    Ok(r) => match crate::http::read_body(r) {
      Ok(b) => b,
      Err(e) => {
        log::warn!("huggingface: abstract batch fetch failed — {e}");
        return;
      }
    },
    Err(e) => {
      log::warn!("huggingface: abstract batch fetch failed — {e}");
      return;
    }
  };

  let abstracts = parse_abstracts(&body);
  log::debug!("huggingface: received {} abstracts from arXiv", abstracts.len());

  for item in items.iter_mut() {
    if let Some(text) = abstracts.get(&item.id) {
      item.summary_short = text.clone();
    } else {
      log::warn!("huggingface: no abstract matched for {}", item.id);
    }
  }
}

/// Parse an arXiv Atom response and return a map of arXiv ID → truncated abstract.
fn parse_abstracts(xml: &str) -> HashMap<String, String> {
  let mut map = HashMap::new();
  let mut reader = Reader::from_str(xml);
  reader.config_mut().trim_text(true);

  let mut in_entry = false;
  let mut current_tag = String::new();
  let mut entry_id = String::new();
  let mut summary = String::new();

  loop {
    match reader.read_event() {
      Ok(XmlEvent::Start(ref e)) => {
        let tag =
          std::str::from_utf8(e.name().as_ref()).unwrap_or("").to_string();
        if tag == "entry" {
          in_entry = true;
          entry_id.clear();
          summary.clear();
        }
        current_tag = tag;
      }

      Ok(XmlEvent::Text(ref e)) => {
        if !in_entry {
          continue;
        }
        let text = e.unescape().unwrap_or_default().to_string();
        match current_tag.as_str() {
          "id" => entry_id.push_str(&text),
          "summary" => summary.push_str(&text),
          _ => {}
        }
      }

      Ok(XmlEvent::End(ref e)) => {
        let tag =
          std::str::from_utf8(e.name().as_ref()).unwrap_or("").to_string();
        if in_entry && tag == "entry" {
          in_entry = false;
          if let Some(id) = extract_arxiv_id(&entry_id) {
            let clean = abstract_collapse_whitespace(summary.trim());
            map.insert(id, abstract_truncate(&clean, 300));
          }
        }
      }

      Ok(XmlEvent::Eof) => break,
      Err(_) => break,
      _ => {}
    }
  }

  map
}

/// Extract a bare arXiv ID from a full URL like `http://arxiv.org/abs/2403.00001v2`.
fn extract_arxiv_id(url: &str) -> Option<String> {
  let last = url.rsplit('/').next()?;
  // Strip version suffix (e.g. "v2").
  let id = match last.rfind('v') {
    Some(pos) if last[pos + 1..].chars().all(|c| c.is_ascii_digit()) => {
      &last[..pos]
    }
    _ => last,
  };
  if id.is_empty() { None } else { Some(id.to_string()) }
}

fn abstract_collapse_whitespace(s: &str) -> String {
  s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn abstract_truncate(s: &str, max: usize) -> String {
  let mut chars = s.chars();
  let mut out = String::new();
  let mut n = 0;
  for c in &mut chars {
    if n >= max {
      if chars.next().is_some() {
        out.push('…');
      }
      break;
    }
    out.push(c);
    n += 1;
  }
  out
}

// ---------------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------------

fn get_text(url: &str) -> Result<String, String> {
  let resp = crate::http::client()
    .get(url)
    .send()
    .map_err(|e| format!("HTTP error: {e}"))?;
  if !resp.status().is_success() {
    return Err(format!("HTTP {}", resp.status()));
  }
  crate::http::read_body(resp)
}

fn today_date() -> String {
  chrono::Utc::now().format("%Y-%m-%d").to_string()
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse the HF daily papers page.
///
/// The page has two sources of data:
/// 1. `<h3><a href="/papers/{id}" ...>TITLE</a></h3>` — one per paper.
/// 2. HTML-entity-encoded JSON blobs in the page body containing:
///    `&quot;id&quot;:&quot;{id}&quot;`
///    `&quot;upvotes&quot;:{n}`
///    `&quot;name&quot;:&quot;AUTHOR&quot;` (within an authors array)
fn parse_papers(html: &str, today: &str) -> Vec<FeedItem> {
  // Pass 1: extract (id, title) from <h3> tags.
  let titles = extract_h3_papers(html);

  // Pass 2: extract upvotes and authors from the encoded JSON blobs.
  let meta = extract_json_meta(html);

  // Merge, preserving the order titles were found.
  let mut seen = std::collections::HashSet::new();
  let mut items = Vec::new();

  for (id, title) in titles {
    if !seen.insert(id.clone()) {
      continue; // deduplicate
    }
    let (upvotes, authors) = meta.get(&id).cloned().unwrap_or((None, vec![]));

    let upvote_count = upvotes.unwrap_or(0);

    // Detect subtopics from title; fall back to generic "ml" tag.
    let subtopics: Vec<String> =
      detect_subtopics(&title, "").into_iter().map(|s| s.to_string()).collect();
    let domain_tags =
      if subtopics.is_empty() { vec!["ml".to_string()] } else { subtopics };

    let mut item = FeedItem {
      id: id.clone(),
      title,
      source_platform: SourcePlatform::HuggingFace,
      content_type: ContentType::Paper,
      domain_tags,
      signal: SignalLevel::Secondary,
      published_at: today.to_string(),
      authors,
      summary_short: String::new(),
      workflow_state: WorkflowState::Inbox,
      url: format!("https://huggingface.co/papers/{id}"),
      upvote_count,
      github_repo: None,
      github_owner: None,
      github_repo_name: None,
      benchmark_results: vec![],
      full_content: None,
      source_name: "huggingface".to_string(),
    };
    item.signal = item.compute_signal();
    items.push(item);
  }

  items
}

// ---------------------------------------------------------------------------
// Pass 1 — titles from <h3> elements
// ---------------------------------------------------------------------------

/// Returns (arxiv_id, title) pairs from `<h3><a href="/papers/{id}">TITLE</a></h3>`.
fn extract_h3_papers(html: &str) -> Vec<(String, String)> {
  let mut results = Vec::new();
  let mut pos = 0;

  while let Some(h3_start) = html[pos..].find("<h3").map(|i| i + pos) {
    // Find the end of the h3 block
    let Some(h3_end_rel) = html[h3_start..].find("</h3>") else {
      break;
    };
    let h3_end = h3_start + h3_end_rel + 5; // include </h3>
    let block = &html[h3_start..h3_end];

    pos = h3_end;

    // Inside the block, find href="/papers/{id}"
    let needle = "href=\"/papers/";
    let Some(href_pos) = block.find(needle) else {
      continue;
    };
    let id_start = href_pos + needle.len();
    let id_raw = &block[id_start..];
    let id_end = id_raw.find('"').unwrap_or(id_raw.len());
    let paper_id = &id_raw[..id_end];

    // Validate: must look like an arXiv ID (digits, dot, digits, optional v+digits)
    if !is_arxiv_id(paper_id) {
      continue;
    }

    // Extract the link text (strip all inner tags)
    let title = strip_tags(block).trim().to_string();
    if title.is_empty() {
      continue;
    }

    results.push((paper_id.to_string(), title));
  }

  results
}

fn is_arxiv_id(s: &str) -> bool {
  // Accept NNNN.NNNNN or NNNN.NNNNNvN
  let s = if let Some(v_pos) = s.find('v') {
    let version = &s[v_pos + 1..];
    if version.chars().all(|c| c.is_ascii_digit()) { &s[..v_pos] } else { s }
  } else {
    s
  };
  let Some(dot) = s.find('.') else { return false };
  let before = &s[..dot];
  let after = &s[dot + 1..];
  before.len() == 4
    && after.len() >= 4
    && before.chars().all(|c| c.is_ascii_digit())
    && after.chars().all(|c| c.is_ascii_digit())
}

fn strip_tags(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  let mut in_tag = false;
  for c in s.chars() {
    match c {
      '<' => in_tag = true,
      '>' => in_tag = false,
      _ if !in_tag => out.push(c),
      _ => {}
    }
  }
  out
}

// ---------------------------------------------------------------------------
// Pass 2 — upvotes + authors from encoded JSON
// ---------------------------------------------------------------------------

/// Returns HashMap<arxiv_id, (Option<upvotes>, Vec<author_name>)>
fn extract_json_meta(
  html: &str,
) -> HashMap<String, (Option<u32>, Vec<String>)> {
  let mut map: HashMap<String, (Option<u32>, Vec<String>)> = HashMap::new();

  // The page embeds server data as HTML-entity-encoded JSON.
  // Pattern per paper block:
  //   &quot;id&quot;:&quot;{id}&quot; ... &quot;upvotes&quot;:{n}
  // Authors appear as &quot;name&quot;:&quot;{name}&quot; within the same block.

  let id_needle = "&quot;id&quot;:&quot;";
  let mut pos = 0;

  while let Some(rel) = html[pos..].find(id_needle) {
    let id_val_start = pos + rel + id_needle.len();
    let Some(id_val_end_rel) = html[id_val_start..].find("&quot;") else {
      pos = id_val_start;
      continue;
    };
    let paper_id = &html[id_val_start..id_val_start + id_val_end_rel];

    if !is_arxiv_id(paper_id) {
      pos = id_val_start + id_val_end_rel;
      continue;
    }
    let paper_id = paper_id.to_string();

    // Take the next ~4 KB as the JSON block for this paper
    let block_end = (id_val_start + 4096).min(html.len());
    let block = &html[id_val_start..block_end];

    let upvotes = extract_upvotes(block);
    let authors = extract_authors(block);

    map.entry(paper_id).or_insert((upvotes, authors));

    pos = id_val_start + id_val_end_rel;
  }

  map
}

fn extract_upvotes(block: &str) -> Option<u32> {
  let needle = "&quot;upvotes&quot;:";
  let start = block.find(needle)? + needle.len();
  let rest = &block[start..];
  let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
  rest[..end].parse().ok()
}

/// Extract author names from `&quot;name&quot;:&quot;{name}&quot;` entries.
/// Stops when it hits a field that indicates we've left the authors array.
fn extract_authors(block: &str) -> Vec<String> {
  let mut authors = Vec::new();
  let needle = "&quot;name&quot;:&quot;";
  let mut pos = 0;

  // Only look within the authors array: stop at the first non-author &quot;name&quot;
  // that follows a field clearly outside author objects (e.g. &quot;title&quot;).
  // Simple heuristic: stop after 20 authors or when we leave the first 2 KB.
  let limit = block.len().min(2048);
  let block = &block[..limit];

  while let Some(rel) = block[pos..].find(needle) {
    let val_start = pos + rel + needle.len();
    let Some(val_end_rel) = block[val_start..].find("&quot;") else {
      break;
    };
    let name = &block[val_start..val_start + val_end_rel];
    // Skip internal/system fields that look like object IDs (24-char hex)
    if name.len() < 64
      && !name.is_empty()
      && !name.chars().all(|c| c.is_ascii_hexdigit())
    {
      authors.push(name.to_string());
    }
    pos = val_start + val_end_rel;
    if authors.len() >= 20 {
      break;
    }
  }

  authors
}

// ---------------------------------------------------------------------------
// Repo enrichment — HuggingFace papers API + abstract fallback
// ---------------------------------------------------------------------------

const MAX_ENRICH_REQUESTS: usize = 20;
const CACHE_TTL_DAYS: i32 = 7;

/// Fetch the GitHub repo URL for an arXiv paper from the HuggingFace papers API.
/// Returns `None` on any failure or when the field is absent / empty.
pub fn fetch_paper_repo(arxiv_id: &str) -> Option<String> {
  let url = format!("https://huggingface.co/api/papers/{}", arxiv_id);
  let resp = crate::http::client().get(&url).send().ok()?;
  if !resp.status().is_success() {
    return None;
  }
  let body = crate::http::read_body(resp).ok()?;
  log::debug!("hf api response for {}: {}", arxiv_id, &body);
  let json: serde_json::Value = serde_json::from_str(&body).ok()?;
  json
    .get("githubRepo")
    .and_then(|v| v.as_str())
    .filter(|s| !s.is_empty() && s.contains("github.com"))
    .map(|s| s.to_string())
}

/// For items that have an arXiv ID and no `github_repo` set, attempt to fill
/// `github_repo`, `github_owner`, and `github_repo_name` via:
///   1. HuggingFace papers API (`/api/papers/{id}`)
///   2. GitHub URL regex over the abstract text (fallback)
///
/// Results are cached at `~/.config/trench/hf_repo_cache.json` with a
/// 7-day TTL so subsequent startups don't re-fetch. Capped at 20 live
/// requests per ingestion cycle; items with the most upvotes are tried first.
pub fn enrich_with_repos(items: &mut Vec<FeedItem>) {
  let mut cache = load_hf_cache();
  let mut request_count: usize = 0;
  let mut enriched: usize = 0;
  let mut attempted: usize = 0;

  // Build candidate list sorted by upvotes descending.
  let mut candidates: Vec<(usize, String, u32)> = items
    .iter()
    .enumerate()
    .filter(|(_, item)| item.github_repo.is_none())
    .filter_map(|(i, item)| {
      let id = extract_paper_arxiv_id(item)?;
      Some((i, id, item.upvote_count))
    })
    .collect();
  candidates.sort_by(|a, b| b.2.cmp(&a.2));

  for (idx, arxiv_id, _) in candidates {
    // Cache hit — apply without a network call.
    if let Some(entry) = cache.get(&arxiv_id) {
      if !hf_cache_stale(entry) {
        if let Some(ref repo) = entry.github_repo.clone() {
          apply_repo(items, idx, repo);
          enriched += 1;
        }
        continue;
      }
    }

    if request_count >= MAX_ENRICH_REQUESTS {
      break;
    }

    attempted += 1;

    // Primary: HuggingFace API. Fallback: regex over abstract.
    let abstract_text = items[idx].summary_short.clone();
    let repo_url = fetch_paper_repo(&arxiv_id)
      .or_else(|| extract_github_from_text(&abstract_text));

    request_count += 1;

    // Cache the outcome (including None — so we don't retry papers with no repo).
    cache.insert(
      arxiv_id,
      HfRepoCacheEntry {
        github_repo: repo_url.clone(),
        cached_at: today_date(),
      },
    );

    if let Some(ref repo) = repo_url {
      apply_repo(items, idx, repo);
      enriched += 1;
    }
  }

  log::debug!(
    "repo enrichment: {enriched}/{attempted} items enriched with github_repo"
  );
  save_hf_cache(&cache);
}

fn apply_repo(items: &mut Vec<FeedItem>, idx: usize, repo: &str) {
  items[idx].github_repo = Some(repo.to_string());
  let (owner, name) = parse_github_owner_repo(repo);
  items[idx].github_owner = owner;
  items[idx].github_repo_name = name;
}

/// Return the arXiv ID for items whose `id` field looks like one (`NNNN.NNNNN`).
fn extract_paper_arxiv_id(item: &FeedItem) -> Option<String> {
  let id = &item.id;
  let dot = id.find('.')?;
  let before = &id[..dot];
  let after = &id[dot + 1..];
  if before.len() == 4
    && after.len() >= 4
    && before.chars().all(|c| c.is_ascii_digit())
    && after.chars().all(|c| c.is_ascii_digit())
  {
    Some(id.clone())
  } else {
    None
  }
}

fn extract_github_from_text(text: &str) -> Option<String> {
  use std::sync::OnceLock;
  static RE: OnceLock<regex::Regex> = OnceLock::new();
  let re = RE.get_or_init(|| {
    regex::Regex::new(r"https?://github\.com/[a-zA-Z0-9_.-]+/[a-zA-Z0-9_.-]+")
      .expect("valid regex")
  });
  re.find(text).map(|m| m.as_str().trim_end_matches('.').to_string())
}

fn parse_github_owner_repo(url: &str) -> (Option<String>, Option<String>) {
  let path = url
    .trim_end_matches('/')
    .strip_prefix("https://github.com/")
    .or_else(|| url.strip_prefix("http://github.com/"))
    .unwrap_or("");
  let mut parts = path.splitn(2, '/');
  let owner = parts.next().filter(|s| !s.is_empty()).map(|s| s.to_string());
  let name = parts.next().filter(|s| !s.is_empty()).map(|s| s.to_string());
  (owner, name)
}

// ---------------------------------------------------------------------------
// Cache
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize)]
struct HfRepoCacheEntry {
  github_repo: Option<String>,
  cached_at: String,
}

fn hf_cache_path() -> Option<PathBuf> {
  let mut p = std::env::var_os("HOME").map(PathBuf::from)?;
  p.push(".config");
  p.push("trench");
  p.push("hf_repo_cache.json");
  Some(p)
}

fn load_hf_cache() -> HashMap<String, HfRepoCacheEntry> {
  let path = match hf_cache_path() {
    Some(p) => p,
    None => return HashMap::new(),
  };
  match fs::read(&path) {
    Ok(b) => serde_json::from_slice(&b).unwrap_or_default(),
    Err(_) => HashMap::new(),
  }
}

fn save_hf_cache(cache: &HashMap<String, HfRepoCacheEntry>) {
  let path = match hf_cache_path() {
    Some(p) => p,
    None => return,
  };
  if let Some(parent) = path.parent() {
    let _ = fs::create_dir_all(parent);
  }
  if let Ok(json) = serde_json::to_vec_pretty(cache) {
    if let Err(e) = fs::write(&path, &json) {
      log::warn!("hf repo cache: save failed — {e}");
    } else {
      crate::store::set_private(&path);
    }
  }
}

fn hf_cache_stale(entry: &HfRepoCacheEntry) -> bool {
  use chrono::NaiveDate;
  let cached = match NaiveDate::parse_from_str(&entry.cached_at, "%Y-%m-%d") {
    Ok(d) => d,
    Err(_) => return true,
  };
  let today = chrono::Utc::now().date_naive();
  (today - cached).num_days() > CACHE_TTL_DAYS as i64
}
