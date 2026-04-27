use crate::models::FeedItem;

const WRAP_WIDTH: usize = 80;

/// Fetch and return plain-text lines for a feed item.
///
/// Fallback chain:
///   1. Cached `full_content` from RSS `<content:encoded>`.
///   2. arXiv HTML render (only for arXiv / HuggingFace paper URLs).
///   3. Readability extraction from the item URL.
///   3b. Chromium headless fetch + readability (skipped if no chromium in PATH).
///   4. `summary_short` as last resort.
pub fn fetch(item: &FeedItem) -> Result<Vec<String>, String> {
  // Step 1: cached full_content from RSS.
  if let Some(ref content) = item.full_content {
    if !content.is_empty() {
      log::debug!(
        "fulltext: step=cached_full_content ({} chars)",
        content.len()
      );
      return Ok(wrap_lines(content, WRAP_WIDTH));
    }
  }

  // Step 2: arXiv HTML.
  if let Some(id) = extract_arxiv_id(&item.url) {
    let html_url = format!("https://arxiv.org/html/{id}");
    log::debug!("fulltext: step=arxiv_html url={html_url}");
    match get_text(&html_url) {
      Ok(html) => {
        log::debug!("fulltext: arxiv_html response {} bytes", html.len());
        let plain = strip_html(&html);
        return Ok(wrap_lines(&plain, WRAP_WIDTH));
      }
      Err(e) => {
        log::warn!(
          "fulltext: arxiv_html failed for {id} — {e}, trying readability"
        );
      }
    }
  }

  // Step 3: readability extraction.
  match fetch_with_readability(&item.url) {
    Ok(text) => {
      log::debug!("fulltext: step=readability ({} chars)", text.len());
      let lines = wrap_lines(&text, WRAP_WIDTH);
      log::debug!("fulltext: readability wrapped_lines={}", lines.len());
      return Ok(lines);
    }
    Err(e) => {
      log::warn!(
        "fulltext: readability failed for {} — {e}, trying chromium",
        item.url
      );
    }
  }

  // Step 3b: chromium headless fetch + readability.
  match chromium_fetch(&item.url) {
    Ok(text) => {
      log::debug!("fulltext: step=chromium ({} chars)", text.len());
      return Ok(wrap_lines(&text, WRAP_WIDTH));
    }
    Err(e) => {
      log::warn!(
        "fulltext: chromium failed for {} — {e}, falling back to summary",
        item.url
      );
    }
  }

  // Step 4: summary fallback.
  log::debug!("fulltext: step=summary_fallback");
  Ok(wrap_lines(&item.summary_short, WRAP_WIDTH))
}

// ---------------------------------------------------------------------------
// Readability extraction
// ---------------------------------------------------------------------------

fn fetch_with_readability(url: &str) -> Result<String, String> {
  let html = get_text(url)?;
  log::debug!("fulltext: readability url={url} raw_html={} bytes", html.len());
  apply_readability(&html, url)
}

fn apply_readability(html: &str, url: &str) -> Result<String, String> {
  let parsed_url =
    url::Url::parse(url).map_err(|e| format!("URL parse error: {e}"))?;
  let product =
    readability::extractor::extract(&mut html.as_bytes(), &parsed_url)
      .map_err(|e| format!("readability error: {e}"))?;
  log::debug!(
    "fulltext: readability extracted {} bytes of article content",
    product.content.len()
  );
  let text = strip_html(&product.content);
  if text.trim().is_empty() {
    return Err("readability returned empty content".to_string());
  }
  Ok(text)
}

// ---------------------------------------------------------------------------
// Chromium headless extraction
// ---------------------------------------------------------------------------

fn find_chromium() -> Option<String> {
  let path_var = std::env::var("PATH").unwrap_or_default();
  for dir in std::env::split_paths(&path_var) {
    for name in &["chromium", "chromium-browser", "google-chrome"] {
      if dir.join(name).is_file() {
        return Some(name.to_string());
      }
    }
  }
  None
}

fn run_with_timeout(
  binary: &str,
  args: &[&str],
  timeout: std::time::Duration,
) -> Result<Vec<u8>, String> {
  use std::io::Read;
  use std::time::Instant;

  let mut child = std::process::Command::new(binary)
    .args(args)
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::null())
    .spawn()
    .map_err(|e| format!("chromium spawn: {e}"))?;

  let mut stdout = child.stdout.take().expect("piped stdout");
  let (tx, rx) = std::sync::mpsc::channel::<Result<Vec<u8>, String>>();
  std::thread::spawn(move || {
    let mut buf = Vec::new();
    let result = stdout
      .read_to_end(&mut buf)
      .map(|_| buf)
      .map_err(|e| format!("read: {e}"));
    tx.send(result).ok();
  });

  let deadline = Instant::now() + timeout;
  loop {
    match child.try_wait().map_err(|e| format!("wait: {e}"))? {
      Some(_) => break,
      None => {
        if Instant::now() >= deadline {
          let _ = child.kill();
          let _ = child.wait();
          return Err("chromium timed out".to_string());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
      }
    }
  }

  rx.recv().map_err(|_| "reader thread disconnected".to_string())?
}

fn chromium_fetch(url: &str) -> Result<String, String> {
  let binary =
    find_chromium().ok_or_else(|| "no chromium in PATH".to_string())?;
  log::debug!("fulltext: chromium binary={binary}");

  let html_bytes = run_with_timeout(
    &binary,
    &["--headless", "--disable-gpu", "--dump-dom", url],
    std::time::Duration::from_secs(10),
  )?;
  log::debug!("fulltext: chromium dom={} bytes for {url}", html_bytes.len());

  let html = String::from_utf8_lossy(&html_bytes);
  let text = apply_readability(&html, url)?;
  log::debug!("fulltext: chromium readability result={} chars", text.len());

  if text.len() > 500 {
    Ok(text)
  } else {
    Err(format!("chromium extraction too short ({} chars)", text.len()))
  }
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
  crate::http::read_body(resp).map_err(|e| format!("Body read error: {e}"))
}

// ---------------------------------------------------------------------------
// arXiv ID extraction
// ---------------------------------------------------------------------------

fn extract_arxiv_id(url: &str) -> Option<&str> {
  if let Some(pos) = url.find("/papers/") {
    let id = &url[pos + "/papers/".len()..];
    let id = id.split('?').next().unwrap_or(id);
    let id = id.split('#').next().unwrap_or(id);
    if !id.is_empty() {
      return Some(id);
    }
  }
  for prefix in ["/abs/", "/html/", "/pdf/"] {
    if let Some(pos) = url.find(prefix) {
      let id = &url[pos + prefix.len()..];
      let id = id.split('?').next().unwrap_or(id);
      let id = id.split('#').next().unwrap_or(id);
      if !id.is_empty() {
        return Some(id);
      }
    }
  }
  None
}

// ---------------------------------------------------------------------------
// HTML → plain text
// ---------------------------------------------------------------------------

fn strip_html(html: &str) -> String {
  let mut out = String::with_capacity(html.len() / 3);
  let mut in_tag = false;
  let mut in_script = false;
  let mut in_style = false;
  let mut tag_buf = String::new();

  let mut chars = html.chars().peekable();

  while let Some(c) = chars.next() {
    if in_tag {
      if c == '>' {
        let tag_lower = tag_buf.trim_start().to_lowercase();
        let tag_name: &str = tag_lower
          .split(|c: char| c.is_whitespace() || c == '/')
          .next()
          .unwrap_or("");

        let is_block = matches!(
          tag_name,
          "p"
            | "div"
            | "br"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "li"
            | "tr"
            | "section"
            | "article"
            | "header"
            | "footer"
            | "main"
            | "blockquote"
        );

        let is_script_open = tag_name == "script";
        let is_script_close = tag_lower.starts_with("/script");
        let is_style_open = tag_name == "style";
        let is_style_close = tag_lower.starts_with("/style");

        if is_script_close {
          in_script = false;
        } else if is_style_close {
          in_style = false;
        } else if is_script_open {
          in_script = true;
        } else if is_style_open {
          in_style = true;
        } else if is_block && !in_script && !in_style {
          out.push('\n');
        }

        tag_buf.clear();
        in_tag = false;
      } else {
        tag_buf.push(c);
      }
    } else if c == '<' {
      in_tag = true;
      tag_buf.clear();
    } else if !in_script && !in_style {
      if c == '&' {
        let mut entity = String::new();
        for ec in chars.by_ref() {
          if ec == ';' {
            break;
          }
          if ec.is_whitespace() {
            out.push('&');
            out.push(ec);
            entity.clear();
            break;
          }
          entity.push(ec);
          if entity.len() > 8 {
            out.push('&');
            out.push_str(&entity);
            entity.clear();
            break;
          }
        }
        if !entity.is_empty() {
          match entity.as_str() {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            "nbsp" => out.push(' '),
            "mdash" | "#8212" => out.push('—'),
            "ndash" | "#8211" => out.push('–'),
            "ldquo" | "#8220" => out.push('"'),
            "rdquo" | "#8221" => out.push('"'),
            "lsquo" | "#8216" => out.push('\u{2018}'),
            "rsquo" | "#8217" => out.push('\u{2019}'),
            "hellip" | "#8230" => out.push('…'),
            _ => {
              out.push('&');
              out.push_str(&entity);
              out.push(';');
            }
          }
        }
      } else {
        out.push(c);
      }
    }
  }

  out
}

// ---------------------------------------------------------------------------
// Post-processing
// ---------------------------------------------------------------------------

fn strip_ansi(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  let mut chars = s.chars().peekable();
  while let Some(c) = chars.next() {
    if c == '\x1b' {
      for ch in chars.by_ref() {
        if matches!(ch, 'm' | 'K' | 'J' | 'H' | 'A' | 'B' | 'C' | 'D') {
          break;
        }
      }
    } else {
      out.push(c);
    }
  }
  out
}

fn wrap_lines(text: &str, width: usize) -> Vec<String> {
  let mut paragraphs: Vec<String> = Vec::new();
  let mut current = String::new();
  let mut blank_run = 0usize;

  for raw_line in text.lines() {
    let stripped = strip_ansi(raw_line);
    let line = stripped.trim();
    if line.is_empty() {
      blank_run += 1;
      if blank_run == 1 && !current.trim().is_empty() {
        paragraphs.push(current.trim().to_string());
        current.clear();
      }
      if blank_run == 1 {
        paragraphs.push(String::new());
      }
    } else {
      blank_run = 0;
      if !current.is_empty() {
        current.push(' ');
      }
      current.push_str(line);
    }
  }
  if !current.trim().is_empty() {
    paragraphs.push(current.trim().to_string());
  }

  let mut out: Vec<String> = Vec::new();
  for para in paragraphs {
    if para.is_empty() {
      out.push(String::new());
      continue;
    }
    let mut line_buf = String::new();
    for word in para.split_whitespace() {
      if line_buf.is_empty() {
        line_buf.push_str(word);
      } else if line_buf.len() + 1 + word.len() <= width {
        line_buf.push(' ');
        line_buf.push_str(word);
      } else {
        out.push(line_buf.clone());
        line_buf.clear();
        line_buf.push_str(word);
      }
    }
    if !line_buf.is_empty() {
      out.push(line_buf);
    }
  }

  out
}
