use std::fs;
use std::path::PathBuf;

use crate::models::FeedItem;

fn cache_path() -> Option<PathBuf> {
  let mut p = std::env::var_os("HOME").map(PathBuf::from)?;
  p.push(".config");
  p.push("trench");
  p.push("cache.json");
  Some(p)
}

pub fn load() -> Vec<FeedItem> {
  let path = match cache_path() {
    Some(p) => p,
    None => return Vec::new(),
  };

  let bytes = match fs::read(&path) {
    Ok(b) => b,
    Err(_) => return Vec::new(),
  };

  let mut items: Vec<FeedItem> =
    serde_json::from_slice(&bytes).unwrap_or_default();
  // Defense-in-depth: items persisted before sanitize-at-ingestion shipped
  // may have raw escape sequences baked into their string fields. Idempotent
  // re-sanitize protects against terminal-hijack via the cache.
  for item in &mut items {
    item.sanitize_in_place();
  }
  items
}

pub fn save(items: &[FeedItem]) {
  let path = match cache_path() {
    Some(p) => p,
    None => return,
  };

  if let Some(parent) = path.parent() {
    let _ = fs::create_dir_all(parent);
  }

  if let Ok(json) = serde_json::to_vec_pretty(items) {
    let _ = super::atomic_write(&path, &json);
    crate::store::set_private(&path);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// End-to-end smoke: a cache file that already contains terminal-escape
  /// sequences in `title` / `summary_short` (e.g. an item ingested before
  /// sanitize-at-ingestion shipped) must be sanitized on load. This protects
  /// against terminal hijack via the cache.
  #[test]
  fn load_sanitizes_escape_sequences_in_existing_cache() {
    use crate::models::{
      ContentType, SignalLevel, SourcePlatform, WorkflowState,
    };

    // Hostile payload: clear-screen CSI in title, OSC 52 (clipboard) in
    // summary, OSC 8 hyperlink in authors.
    let raw_item = FeedItem {
      id: "test-1".into(),
      title: "Paper Title\x1b[2J\x1b[Hhijack".into(),
      source_platform: SourcePlatform::ArXiv,
      content_type: ContentType::Paper,
      domain_tags: vec!["legit\x1b]52;c;evil\x07tag".into()],
      signal: SignalLevel::Primary,
      published_at: "2026-05-04".into(),
      authors: vec!["A\x1b]8;;https://evil/\x1b\\Real\x1b]8;;\x1b\\B".into()],
      summary_short: "Abstract\x1b[31mred\x1b[0m end".into(),
      workflow_state: WorkflowState::Inbox,
      url: "http://arxiv.org/abs/test-1".into(),
      upvote_count: 0,
      github_repo: None,
      github_owner: None,
      github_repo_name: None,
      benchmark_results: vec![],
      full_content: Some("body\x1b[2Kclear".into()),
      source_name: "arxiv".into(),
      title_lower: String::new(),
      authors_lower: Vec::new(),
    };
    let json = serde_json::to_vec_pretty(&vec![raw_item]).unwrap();

    // Set HOME to a temp dir so cache_path() resolves there. Running this in
    // serial would be safer; for a single test it's fine since other tests
    // don't rely on $HOME.
    let dir = std::env::temp_dir().join(format!(
      "trench_cache_load_test_{}",
      std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".config/trench")).unwrap();
    std::fs::write(dir.join(".config/trench/cache.json"), &json).unwrap();

    // SAFETY: tests run in a single-process address space; we override HOME
    // for the duration of this test only. Acceptable risk inside a #[cfg(test)]
    // block that doesn't run in parallel with other HOME-dependent tests.
    let prev_home = std::env::var_os("HOME");
    unsafe {
      std::env::set_var("HOME", &dir);
    }

    let items = load();

    // Restore HOME before any assertion can fail and skip the cleanup.
    unsafe {
      match prev_home {
        Some(v) => std::env::set_var("HOME", v),
        None => std::env::remove_var("HOME"),
      }
    }
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(items.len(), 1);
    let it = &items[0];
    assert!(!it.title.contains('\x1b'), "title still has ESC: {:?}", it.title);
    assert!(
      !it.summary_short.contains('\x1b'),
      "summary still has ESC: {:?}",
      it.summary_short
    );
    assert!(
      it.authors.iter().all(|a| !a.contains('\x1b')),
      "authors still have ESC: {:?}",
      it.authors
    );
    assert!(
      it.domain_tags.iter().all(|t| !t.contains('\x1b')),
      "tags still have ESC: {:?}",
      it.domain_tags
    );
    assert!(
      it.full_content.as_deref().is_none_or(|s| !s.contains('\x1b')),
      "full_content still has ESC: {:?}",
      it.full_content
    );
    // Visible text is preserved.
    assert!(it.title.contains("Paper Title"));
    assert!(it.title.contains("hijack"));
    assert!(it.summary_short.contains("Abstract"));
    assert!(it.summary_short.contains("end"));
  }
}
