use quick_xml::Reader;
use quick_xml::events::Event as XmlEvent;

use crate::models::{
  ContentType, FeedItem, SignalLevel, SourcePlatform, WorkflowState,
  detect_subtopics,
};

pub fn fetch(
  source_name: &str,
  feed_url: &str,
  platform: SourcePlatform,
  content_type: ContentType,
) -> Result<Vec<FeedItem>, String> {
  log::info!("rss {source_name}: fetching {feed_url}");

  let resp = reqwest::blocking::get(feed_url).map_err(|e| {
    let msg = format!("{source_name}: HTTP request failed: {e}");
    log::error!("{msg}");
    msg
  })?;

  let status = resp.status();
  if !status.is_success() {
    let msg = format!("{source_name}: HTTP {status}");
    log::warn!("{msg}");
    return Err(msg);
  }

  let body = resp.text().map_err(|e| {
    let msg = format!("{source_name}: Failed to read response: {e}");
    log::error!("{msg}");
    msg
  })?;

  log::debug!("rss {source_name}: response {} bytes", body.len());

  let items =
    parse_feed(&body, platform, content_type, source_name).map_err(|e| {
      let msg = format!("{source_name}: {e}");
      log::error!("{msg}");
      msg
    })?;

  log::info!("rss {source_name}: parsed {} items", items.len());
  Ok(items)
}

// ---------------------------------------------------------------------------
// Feed parser — handles both RSS 2.0 and Atom
// ---------------------------------------------------------------------------

fn parse_feed(
  xml: &str,
  platform: SourcePlatform,
  content_type: ContentType,
  source_name: &str,
) -> Result<Vec<FeedItem>, String> {
  let mut reader = Reader::from_str(xml);
  reader.config_mut().trim_text(true);

  let mut items: Vec<FeedItem> = Vec::new();

  let mut in_entry = false; // inside <entry> or <item>
  let mut in_author = false; // inside Atom <author> container
  let mut current_tag = String::new();

  let mut title = String::new();
  let mut url = String::new();
  let mut published_at = String::new();
  let mut summary = String::new();
  let mut author = String::new();
  let mut content_encoded = String::new();

  loop {
    match reader.read_event() {
      Ok(XmlEvent::Start(ref e)) => {
        let tag = local_name(e.name().as_ref());
        match tag.as_str() {
          "entry" | "item" => {
            in_entry = true;
            in_author = false;
            title.clear();
            url.clear();
            published_at.clear();
            summary.clear();
            author.clear();
            content_encoded.clear();
          }
          "author" if in_entry => {
            in_author = true;
          }
          _ => {}
        }
        current_tag = tag;
      }

      Ok(XmlEvent::Empty(ref e)) => {
        // Atom: <link href="..." rel="alternate"/>
        let tag = local_name(e.name().as_ref());
        if in_entry && tag == "link" && url.is_empty() {
          for attr in e.attributes().flatten() {
            if attr.key.as_ref() == b"href" {
              if let Ok(val) = std::str::from_utf8(&attr.value) {
                url = val.to_string();
              }
            }
          }
        }
      }

      Ok(XmlEvent::Text(ref e)) => {
        if !in_entry {
          continue;
        }
        let text = e.unescape().unwrap_or_default();
        let text = text.trim().to_string();
        if text.is_empty() {
          continue;
        }

        if in_author && current_tag == "name" {
          // Atom: <author><name>text</name></author>
          author = text;
        } else {
          match current_tag.as_str() {
            "title" => title.push_str(&text),
            // RSS 2.0: <link> contains the URL as text
            "link" if url.is_empty() => url.push_str(&text),
            "pubDate" | "published" => {
              if published_at.is_empty() {
                published_at = text;
              }
            }
            "description" | "summary" => summary.push_str(&text),
            // RSS <author> text or <dc:creator>
            "author" | "creator" => {
              if author.is_empty() {
                author = text;
              }
            }
            "encoded" => content_encoded.push_str(&text),
            _ => {}
          }
        }
      }

      // CDATA sections (e.g. <title><![CDATA[Some Title]]></title>) are
      // delivered as a separate event type — handle identically to Text.
      Ok(XmlEvent::CData(ref e)) => {
        if !in_entry {
          continue;
        }
        let text = String::from_utf8_lossy(e.as_ref()).trim().to_string();
        if text.is_empty() {
          continue;
        }

        if in_author && current_tag == "name" {
          author = text;
        } else {
          match current_tag.as_str() {
            "title" => title.push_str(&text),
            "link" if url.is_empty() => url.push_str(&text),
            "pubDate" | "published" => {
              if published_at.is_empty() {
                published_at = text;
              }
            }
            "description" | "summary" => summary.push_str(&text),
            "author" | "creator" => {
              if author.is_empty() {
                author = text;
              }
            }
            "encoded" => content_encoded.push_str(&text),
            _ => {}
          }
        }
      }

      Ok(XmlEvent::End(ref e)) => {
        let tag = local_name(e.name().as_ref());

        if in_entry && tag == "author" {
          in_author = false;
        }

        if in_entry && (tag == "entry" || tag == "item") {
          in_entry = false;
          current_tag.clear();

          if title.is_empty() || url.is_empty() {
            if title.is_empty() {
              log::warn!(
                "rss {source_name}: skipping entry — missing title (url={:?})",
                url
              );
            } else {
              log::warn!(
                "rss {source_name}: skipping entry — missing url (title={:?})",
                title
              );
            }
            continue;
          }

          let clean_title = collapse_whitespace(&title);
          let raw_summary = strip_html(&collapse_whitespace(&summary));
          let summary_short = truncate_chars(&raw_summary, 300);
          let date = normalise_date(&published_at);
          let authors = if author.trim().is_empty() {
            vec![]
          } else {
            vec![author.trim().to_string()]
          };

          // Detect subtopics from title and summary; fall back to generic tag.
          let subtopics: Vec<String> =
            detect_subtopics(&clean_title, &raw_summary)
              .into_iter()
              .map(|s| s.to_string())
              .collect();
          let domain_tags = if subtopics.is_empty() {
            vec!["ml".to_string()]
          } else {
            subtopics
          };

          // Extract full content from <content:encoded> if substantial.
          let full_content = if content_encoded.len() > 500 {
            Some(strip_html(&content_encoded))
          } else {
            None
          };

          let mut item = FeedItem {
            id: url.clone(),
            title: clean_title,
            source_platform: platform.clone(),
            content_type: content_type.clone(),
            domain_tags,
            signal: SignalLevel::Secondary,
            published_at: date,
            authors,
            summary_short,
            workflow_state: WorkflowState::Inbox,
            url: url.clone(),
            upvote_count: 0,
            github_repo: None,
            github_owner: None,
            github_repo_name: None,
            benchmark_results: vec![],
            full_content,
            source_name: source_name.to_string(),
          };
          item.signal = item.compute_signal();
          items.push(item);

          title.clear();
          url.clear();
          published_at.clear();
          summary.clear();
          author.clear();
          content_encoded.clear();
        }
      }

      Ok(XmlEvent::Eof) => break,
      Err(e) => return Err(format!("XML parse error: {e}")),
      _ => {}
    }
  }

  Ok(items)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Strip namespace prefix: "dc:creator" → "creator".
fn local_name(raw: &[u8]) -> String {
  let s = std::str::from_utf8(raw).unwrap_or("");
  match s.rfind(':') {
    Some(pos) => s[pos + 1..].to_string(),
    None => s.to_string(),
  }
}

fn collapse_whitespace(s: &str) -> String {
  s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Remove HTML tags, replacing closing block tags with a space.
fn strip_html(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  let mut in_tag = false;
  for c in s.chars() {
    match c {
      '<' => in_tag = true,
      '>' => {
        in_tag = false;
        out.push(' ');
      }
      _ if !in_tag => out.push(c),
      _ => {}
    }
  }
  out
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

/// Normalise various date formats to YYYY-MM-DD.
///
/// Handles:
/// - ISO 8601 / Atom: `2026-03-15T00:00:00Z` → `2026-03-15`
/// - RFC 2822 / RSS:  `Mon, 15 Mar 2026 00:00:00 GMT` → `2026-03-15`
fn normalise_date(s: &str) -> String {
  let s = s.trim();
  if s.is_empty() {
    return String::new();
  }
  // ISO 8601: starts with YYYY-
  if s.len() >= 10 && s.as_bytes().get(4) == Some(&b'-') {
    return s[..10].to_string();
  }
  // RFC 2822: strip optional "Day, " prefix
  let s = match s.find(',') {
    Some(pos) => s[pos + 1..].trim_start(),
    None => s,
  };
  // Expect: DD Mon YYYY ...
  let parts: Vec<&str> = s.split_whitespace().collect();
  if parts.len() < 3 {
    return String::new();
  }
  let day_n: u32 = parts[0].parse().unwrap_or(0);
  if day_n == 0 {
    return String::new();
  }
  let month = match parts[1] {
    "Jan" => "01",
    "Feb" => "02",
    "Mar" => "03",
    "Apr" => "04",
    "May" => "05",
    "Jun" => "06",
    "Jul" => "07",
    "Aug" => "08",
    "Sep" => "09",
    "Oct" => "10",
    "Nov" => "11",
    "Dec" => "12",
    _ => return String::new(),
  };
  let year = parts[2];
  format!("{year}-{month}-{day_n:02}")
}
