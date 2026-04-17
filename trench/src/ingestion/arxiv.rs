use quick_xml::Reader;
use quick_xml::events::Event;

use crate::models::{
  ContentType, FeedItem, SignalLevel, SourcePlatform, WorkflowState,
  detect_subtopics, map_arxiv_category,
};

pub fn fetch(categories: &[String]) -> Result<Vec<FeedItem>, String> {
  let query = if categories.is_empty() {
    "cat:cs.LG+OR+cat:cs.AI+OR+cat:stat.ML".to_string()
  } else {
    categories
      .iter()
      .map(|c| format!("cat:{}", c))
      .collect::<Vec<_>>()
      .join("+OR+")
  };
  let url = format!(
    "http://export.arxiv.org/api/query\
     ?search_query={query}\
     &sortBy=submittedDate&sortOrder=descending&max_results=50"
  );
  let body = reqwest::blocking::get(&url)
    .map_err(|e| format!("HTTP request failed: {e}"))?
    .text()
    .map_err(|e| format!("Failed to read response body: {e}"))?;

  parse_atom(&body)
}

/// Free-text arXiv search using `search_query=all:{query}`.
pub fn search_query(
  query: &str,
  max_results: usize,
) -> Result<Vec<FeedItem>, String> {
  let encoded = encode_arxiv_query(query);
  let url = format!(
    "http://export.arxiv.org/api/query\
     ?search_query=all:{encoded}\
     &sortBy=submittedDate&sortOrder=descending&max_results={}",
    max_results.min(100)
  );
  let body = reqwest::blocking::get(&url)
    .map_err(|e| format!("HTTP request failed: {e}"))?
    .text()
    .map_err(|e| format!("Failed to read response body: {e}"))?;

  parse_atom(&body)
}

/// Fetch specific papers by arXiv ID list.
pub fn fetch_by_ids(ids: &[String]) -> Result<Vec<FeedItem>, String> {
  let normalized: Vec<String> =
    ids.iter().filter_map(|id| normalize_arxiv_id(id)).collect();
  if normalized.is_empty() {
    return Ok(Vec::new());
  }

  let id_list = normalized.join(",");
  let url = format!(
    "http://export.arxiv.org/api/query?id_list={id_list}&max_results={}",
    normalized.len().min(100)
  );
  let body = reqwest::blocking::get(&url)
    .map_err(|e| format!("HTTP request failed: {e}"))?
    .text()
    .map_err(|e| format!("Failed to read response body: {e}"))?;

  parse_atom(&body)
}

// ---------------------------------------------------------------------------
// Atom XML parser
// ---------------------------------------------------------------------------

fn parse_atom(xml: &str) -> Result<Vec<FeedItem>, String> {
  let mut reader = Reader::from_str(xml);
  reader.config_mut().trim_text(true);

  let mut items: Vec<FeedItem> = Vec::new();

  // Per-entry accumulator
  let mut in_entry = false;
  let mut current_tag = String::new();
  let mut in_author = false;

  let mut title = String::new();
  let mut url = String::new();
  let mut published_at = String::new();
  let mut summary = String::new();
  let mut authors: Vec<String> = Vec::new();
  let mut current_author_name = String::new();
  let mut domain_tags: Vec<String> = Vec::new();

  loop {
    match reader.read_event() {
      Ok(Event::Start(ref e)) => {
        let tag =
          std::str::from_utf8(e.name().as_ref()).unwrap_or("").to_string();

        match tag.as_str() {
          "entry" => {
            in_entry = true;
            // Reset accumulators
            title.clear();
            url.clear();
            published_at.clear();
            summary.clear();
            authors.clear();
            domain_tags.clear();
          }
          "author" if in_entry => {
            in_author = true;
            current_author_name.clear();
          }
          _ => {}
        }
        current_tag = tag;
      }

      Ok(Event::Empty(ref e)) => {
        let name = e.name();
        let tag = std::str::from_utf8(name.as_ref()).unwrap_or("");
        if in_entry && tag == "category" {
          // <category term="cs.LG" scheme="..."/>
          for attr in e.attributes().flatten() {
            if attr.key.as_ref() == b"term" {
              if let Ok(val) = std::str::from_utf8(&attr.value) {
                domain_tags.push(val.to_string());
              }
            }
          }
        }
      }

      Ok(Event::Text(ref e)) => {
        let text = e.unescape().unwrap_or_default().to_string();
        if !in_entry {
          continue;
        }
        if in_author && current_tag == "name" {
          current_author_name.push_str(&text);
        } else {
          match current_tag.as_str() {
            "title" => title.push_str(&text),
            "id" => url.push_str(&text),
            "published" => published_at.push_str(&text),
            "summary" => summary.push_str(&text),
            _ => {}
          }
        }
      }

      Ok(Event::End(ref e)) => {
        let tag =
          std::str::from_utf8(e.name().as_ref()).unwrap_or("").to_string();

        if in_entry && in_author && tag == "name" {
          // name closed inside author — nothing extra needed
        }

        if in_entry && tag == "author" {
          in_author = false;
          let name = current_author_name.trim().to_string();
          if !name.is_empty() {
            authors.push(name);
          }
          current_author_name.clear();
        }

        if in_entry && tag == "entry" {
          in_entry = false;
          current_tag.clear();

          let clean_title = collapse_whitespace(&title);
          let summary_short =
            truncate_chars(&collapse_whitespace(&summary), 300);
          // arXiv published_at looks like "2026-03-15T00:00:00Z" — keep date only
          let date =
            published_at.split('T').next().unwrap_or(&published_at).to_string();

          // Map raw category codes to human-readable labels.
          let mut mapped: Vec<String> = domain_tags
            .iter()
            .filter_map(|code| map_arxiv_category(code.as_str()))
            .map(|label| label.to_string())
            .collect();
          // Append subtopics detected from title and summary.
          for label in detect_subtopics(&clean_title, &summary_short) {
            let s = label.to_string();
            if !mapped.contains(&s) {
              mapped.push(s);
            }
          }

          let mut item = FeedItem {
            id: url.clone(),
            title: clean_title,
            source_platform: SourcePlatform::ArXiv,
            content_type: ContentType::Paper,
            domain_tags: mapped,
            signal: SignalLevel::Primary,
            published_at: date,
            authors,
            summary_short,
            workflow_state: WorkflowState::Inbox,
            url,
            upvote_count: 0,
            github_repo: None,
            github_owner: None,
            github_repo_name: None,
            benchmark_results: vec![],
            full_content: None,
            source_name: "arxiv".to_string(),
          };
          item.signal = item.compute_signal();
          items.push(item);

          // Reset accumulators for next entry
          title = String::new();
          url = String::new();
          published_at = String::new();
          summary = String::new();
          authors = Vec::new();
          domain_tags = Vec::new();
        }
      }

      Ok(Event::Eof) => break,
      Err(e) => return Err(format!("XML parse error: {e}")),
      _ => {}
    }
  }

  Ok(items)
}

fn collapse_whitespace(s: &str) -> String {
  s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_chars(s: &str, max: usize) -> String {
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

fn encode_arxiv_query(query: &str) -> String {
  query
    .split_whitespace()
    .map(percent_encode_query_part)
    .collect::<Vec<_>>()
    .join("+AND+")
}

fn percent_encode_query_part(part: &str) -> String {
  let mut out = String::new();
  for b in part.bytes() {
    let c = b as char;
    if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
      out.push(c);
    } else {
      out.push_str(&format!("%{b:02X}"));
    }
  }
  out
}

fn normalize_arxiv_id(value: &str) -> Option<String> {
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
