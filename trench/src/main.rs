mod app;
mod commands;
mod config;
mod discovery;
mod github;
mod ingestion;
mod keys;
mod models;
mod store;
mod syntax;
pub mod theme;
mod ui;
mod workflows;

use app::{App, DiscoverResult, PaneId, RepoFetchResult};
use crossterm::{
  event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind,
    MouseButton, MouseEventKind,
  },
  execute,
  terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode,
  },
};
use ingestion::message::FetchMessage;
use models::{ContentType, SourcePlatform};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::sync::mpsc;

pub(crate) fn open_url(url: &str) {
  #[cfg(target_os = "macos")]
  let _ = std::process::Command::new("open").arg(url).spawn();
  #[cfg(not(target_os = "macos"))]
  let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

pub(crate) fn truncate_for_notif(s: &str, max: usize) -> String {
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

fn spawn_fetch(tx: mpsc::Sender<FetchMessage>, config: config::Config) {
  std::thread::spawn(move || {
    let enabled = |name: &str| -> bool {
      config.sources.enabled_sources.get(name).copied().unwrap_or(true)
    };

    let mut all_items: Vec<models::FeedItem> = Vec::new();

    log::info!("source arxiv: starting fetch");
    match ingestion::arxiv::fetch(&config.sources.arxiv_categories) {
      Ok(items) => {
        log::info!("source arxiv: completed, {} items", items.len());
        all_items.extend(items.clone());
        let _ = tx.send(FetchMessage::Items(items));
        let _ = tx.send(FetchMessage::SourceComplete("arxiv".to_string()));
      }
      Err(e) => {
        log::error!("source arxiv: failed — {e}");
        let _ = tx.send(FetchMessage::SourceError("arxiv".to_string(), e));
      }
    }

    if enabled("huggingface") {
      log::info!("source huggingface: starting fetch");
      match ingestion::huggingface::fetch() {
        Ok(items) => {
          log::info!("source huggingface: completed, {} items", items.len());
          all_items.extend(items.clone());
          let _ = tx.send(FetchMessage::Items(items));
          let _ =
            tx.send(FetchMessage::SourceComplete("huggingface".to_string()));
        }
        Err(e) => {
          log::error!("source huggingface: failed — {e}");
          let _ =
            tx.send(FetchMessage::SourceError("huggingface".to_string(), e));
        }
      }
    } else {
      log::info!("source huggingface: disabled — skipping");
      let _ = tx.send(FetchMessage::SourceComplete("huggingface".to_string()));
    }

    log::warn!(
      "source anthropic: no RSS feed available; skipping \
       (https://www.anthropic.com/news has no feed link)"
    );

    let rss_feeds: &[(&str, &str, SourcePlatform, ContentType)] = &[
      (
        "openai",
        "https://openai.com/blog/rss.xml",
        SourcePlatform::Blog,
        ContentType::Article,
      ),
      (
        "deepmind",
        "https://deepmind.google/blog/rss.xml",
        SourcePlatform::Blog,
        ContentType::Article,
      ),
      (
        "import_ai",
        "https://importai.substack.com/feed",
        SourcePlatform::Newsletter,
        ContentType::Digest,
      ),
      (
        "bair",
        "https://bair.berkeley.edu/blog/feed.xml",
        SourcePlatform::Blog,
        ContentType::Article,
      ),
      (
        "mit_news_ai",
        "https://news.mit.edu/rss/topic/machine-learning",
        SourcePlatform::Blog,
        ContentType::Article,
      ),
    ];

    for (name, url, platform, content_type) in rss_feeds {
      if !enabled(name) {
        log::info!("source {name}: disabled — skipping");
        let _ = tx.send(FetchMessage::SourceComplete(name.to_string()));
        continue;
      }
      log::info!("source {name}: starting fetch");
      match ingestion::rss::fetch(
        name,
        url,
        platform.clone(),
        content_type.clone(),
      ) {
        Ok(items) => {
          log::info!("source {name}: completed, {} items", items.len());
          all_items.extend(items.clone());
          let _ = tx.send(FetchMessage::Items(items));
          let _ = tx.send(FetchMessage::SourceComplete(name.to_string()));
        }
        Err(e) => {
          log::error!("source {name}: failed — {e}");
          let _ = tx.send(FetchMessage::SourceError(name.to_string(), e));
        }
      }
    }

    // Custom feeds from config.
    for feed in &config.sources.custom_feeds {
      log::info!("source {}: starting fetch (custom)", feed.name);
      match ingestion::rss::fetch(
        &feed.name,
        &feed.url,
        SourcePlatform::Rss,
        ContentType::Article,
      ) {
        Ok(items) => {
          log::info!("source {}: completed, {} items", feed.name, items.len());
          all_items.extend(items.clone());
          let _ = tx.send(FetchMessage::Items(items));
          let _ = tx.send(FetchMessage::SourceComplete(feed.name.clone()));
        }
        Err(e) => {
          log::error!("source {}: failed — {e}", feed.name);
          let _ = tx.send(FetchMessage::SourceError(feed.name.clone(), e));
        }
      }
    }

    log::warn!(
      "source pwc: disabled — API redirects to huggingface.co (returns HTML, not JSON)"
    );
    let _ = tx.send(FetchMessage::SourceComplete("pwc".to_string()));

    log::info!(
      "background: {} total items collected across all sources",
      all_items.len()
    );

    let mut ecache = store::enrichment_cache::load();
    ingestion::semantic_scholar::enrich(
      &mut all_items,
      &mut ecache,
      config.semantic_scholar_key.as_deref(),
    );
    ingestion::huggingface::enrich_with_repos(&mut all_items);
    let with_repo =
      all_items.iter().filter(|i| i.github_repo.is_some()).count();
    log::debug!(
      "ingestion complete: {with_repo}/{} items have github_repo set",
      all_items.len()
    );
    let _ = tx.send(FetchMessage::Items(all_items));
    let _ = tx.send(FetchMessage::SourceComplete("enriching".to_string()));
    let _ = tx.send(FetchMessage::AllComplete);
  });
}

// ── URL discovery pipeline ────────────────────────────────────────────────

pub(crate) fn spawn_discovery(
  url: String,
  tx: std::sync::mpsc::Sender<DiscoverResult>,
) {
  std::thread::spawn(move || {
    let _ = tx.send(discover_feed(&url));
  });
}

fn discover_feed(url: &str) -> DiscoverResult {
  // Step 1: arXiv category patterns.
  for prefix in &[
    "arxiv.org/list/",
    "arxiv.org/abs/",
    "arxiv.org/rss/",
    "export.arxiv.org/rss/",
  ] {
    if let Some(pos) = url.find(prefix) {
      let rest = &url[pos + prefix.len()..];
      let code: String = rest
        .chars()
        .take_while(|&c| c != '/' && c != '?' && c != '#' && c != ' ')
        .collect();
      // Looks like a category code if it contains a dot or is short (cs.LG, stat.ML …)
      if !code.is_empty() && (code.contains('.') || code.len() <= 8) {
        return DiscoverResult::ArxivCategory(code);
      }
    }
  }

  // Step 2: HuggingFace.
  if url.contains("huggingface.co/papers")
    || url.contains("huggingface.co/daily-papers")
  {
    return DiscoverResult::HuggingFaceAlreadyEnabled;
  }

  // Step 3: Substack — derive RSS URL from subdomain.
  if url.contains(".substack.com") {
    let stripped =
      url.trim_start_matches("https://").trim_start_matches("http://");
    let subdomain = stripped.split('.').next().unwrap_or("feed");
    let feed_url = format!("https://{subdomain}.substack.com/feed");
    return DiscoverResult::RssFeed {
      url: feed_url,
      name: subdomain.to_string(),
    };
  }

  let client = reqwest::blocking::Client::builder()
    .timeout(std::time::Duration::from_secs(10))
    .build()
    .unwrap_or_default();

  let base_url = url.trim_end_matches('/').to_string();

  // Step 4: Fetch page and scan <head> for RSS link element.
  if let Ok(resp) = client.get(url).send() {
    if resp.status().is_success() {
      if let Ok(body) = resp.text() {
        if let Some(feed_url) = extract_rss_link(&body, &base_url) {
          let name = domain_name(url);
          return DiscoverResult::RssFeed { url: feed_url, name };
        }
      }
    }
  }

  // Step 5: Try common feed paths.
  let suffixes = ["/feed", "/rss", "/atom.xml", "/feed.xml", "/rss.xml"];
  for suffix in suffixes {
    let candidate = format!("{base_url}{suffix}");
    if let Ok(resp) = client.head(&candidate).send() {
      if resp.status().is_success() {
        let name = domain_name(&candidate);
        return DiscoverResult::RssFeed { url: candidate, name };
      }
    }
  }

  // Step 6: Failure.
  let tried = suffixes
    .iter()
    .map(|s| format!("{base_url}{s}"))
    .collect::<Vec<_>>()
    .join(", ");
  DiscoverResult::Failed(format!("Could not find a feed. Tried: {tried}"))
}

/// Scan HTML for `<link rel="alternate" type="application/rss+xml" href="...">`.
fn extract_rss_link(html: &str, base_url: &str) -> Option<String> {
  let needle = "application/rss+xml";
  let mut search = html;
  while let Some(pos) = search.find(needle) {
    let tag_start = search[..pos].rfind('<').unwrap_or(0);
    let tag_end =
      search[pos..].find('>').map(|p| pos + p + 1).unwrap_or(search.len());
    let tag = &search[tag_start..tag_end];
    if let Some(href) = attr_value(tag, "href") {
      let url = if href.starts_with("http") {
        href
      } else if href.starts_with('/') {
        let origin = url_origin(base_url);
        format!("{origin}{href}")
      } else {
        format!("{base_url}/{href}")
      };
      return Some(url);
    }
    search = &search[pos + needle.len()..];
  }
  None
}

/// Extract the value of a named attribute from a tag string.
fn attr_value(tag: &str, attr: &str) -> Option<String> {
  let needle = format!("{attr}=");
  let pos = tag.find(&needle)?;
  let rest = &tag[pos + needle.len()..];
  if rest.starts_with('"') {
    let end = rest[1..].find('"')?;
    Some(rest[1..end + 1].to_string())
  } else if rest.starts_with('\'') {
    let end = rest[1..].find('\'')?;
    Some(rest[1..end + 1].to_string())
  } else {
    let end =
      rest.find(|c: char| c.is_whitespace() || c == '>').unwrap_or(rest.len());
    Some(rest[..end].to_string())
  }
}

/// Extract `https://host` from a URL.
fn url_origin(url: &str) -> String {
  let stripped =
    url.trim_start_matches("https://").trim_start_matches("http://");
  let host = stripped.split('/').next().unwrap_or("");
  if url.starts_with("https://") {
    format!("https://{host}")
  } else {
    format!("http://{host}")
  }
}

/// Derive a short source name from a URL (e.g. `"openai"` from `openai.com/…`).
fn domain_name(url: &str) -> String {
  let stripped =
    url.trim_start_matches("https://").trim_start_matches("http://");
  let host = stripped.split('/').next().unwrap_or("");
  let host = host.strip_prefix("www.").unwrap_or(host);
  host.split('.').next().unwrap_or(host).to_string()
}

// ── Refresh helper ────────────────────────────────────────────────────────

/// Spawn a fresh fetch cycle and attach the receiver to `app`.
pub(crate) fn do_refresh(app: &mut App) {
  if app.is_loading || app.is_refreshing {
    return;
  }
  let (tx, rx) = mpsc::channel::<FetchMessage>();
  app.fetch_rx = Some(rx);
  let mut sources = vec![
    "arxiv".to_string(),
    "huggingface".to_string(),
    "openai".to_string(),
    "deepmind".to_string(),
    "import_ai".to_string(),
    "bair".to_string(),
    "mit_news_ai".to_string(),
    "pwc".to_string(),
    "enriching".to_string(),
  ];
  for feed in &app.config.sources.custom_feeds {
    sources.push(feed.name.clone());
  }
  app.loading_sources = sources;
  app.is_loading = true;
  app.is_refreshing = true;
  spawn_fetch(tx, app.config.clone());
}

/// Like do_refresh, but always runs — reloads config from disk, abandons any
/// in-flight fetch, clears the item cache, then starts a fresh fetch.
pub(crate) fn force_refresh(app: &mut App) {
  app.config = config::Config::load();
  app.is_loading = false;
  app.is_refreshing = false;
  app.fetch_rx = None;
  app.items.clear();
  do_refresh(app);
}

/// Returns true if enough time has elapsed since the last keyboard scroll.
/// Updates `last_scroll_time` on success.
pub(crate) fn kbd_scroll_ok(app: &mut app::App) -> bool {
  let now = std::time::Instant::now();
  if let Some(last) = app.last_scroll_time {
    if last.elapsed().as_millis() < app.scroll_debounce_ms as u128 {
      return false;
    }
  }
  app.last_scroll_time = Some(now);
  true
}

/// Returns true if enough time has elapsed since the last mouse scroll.
/// Uses a higher debounce threshold to tame trackpad inertia.
fn mouse_scroll_ok(app: &mut app::App) -> bool {
  let now = std::time::Instant::now();
  if let Some(last) = app.last_mouse_scroll_time {
    if last.elapsed().as_millis() < app.mouse_scroll_debounce_ms as u128 {
      return false;
    }
  }
  app.last_mouse_scroll_time = Some(now);
  true
}

fn handle_mouse(
  mouse: crossterm::event::MouseEvent,
  app: &mut app::App,
  terminal: &ratatui::Terminal<ratatui::backend::CrosstermBackend<io::Stdout>>,
) {
  if app.view != app::AppView::Feed {
    return;
  }
  let Ok(size) = terminal.size() else { return };

  // Geometry for scrollbar hit-test.
  let scrollbar_col = size.width.saturating_sub(ui::RIGHT_COL_WIDTH + 1);
  let track_top = 4u16;
  let track_bottom = size.height.saturating_sub(6);

  // Which pane is the cursor in right now?
  let hovered = app.pane_at(mouse.column, mouse.row);

  match mouse.kind {
    // ── Scroll wheel / trackpad ────────────────────────────────────────────
    MouseEventKind::ScrollDown => {
      if mouse_scroll_ok(app) {
        match hovered {
          Some(PaneId::Details) => app.details_scroll_down(),
          Some(PaneId::Notes) => {
            if let Some(notes_app) = app.notes_app.as_mut() {
              notes_app.select_next_note();
            }
          }
          Some(PaneId::Chat) => {
            if let Some(chat_ui) = app.chat_ui.as_mut() {
              chat_ui.scroll_offset = chat_ui.scroll_offset.saturating_add(3);
            }
          }
          _ => {
            if app.filter_focus {
              app.filter_cursor_down();
            } else {
              app.move_down();
            }
          }
        }
      }
    }
    MouseEventKind::ScrollUp => {
      if mouse_scroll_ok(app) {
        match hovered {
          Some(PaneId::Details) => app.details_scroll_up(),
          Some(PaneId::Notes) => {
            if let Some(notes_app) = app.notes_app.as_mut() {
              notes_app.select_prev_note();
            }
          }
          Some(PaneId::Chat) => {
            if let Some(chat_ui) = app.chat_ui.as_mut() {
              chat_ui.scroll_offset = chat_ui.scroll_offset.saturating_sub(3);
            }
          }
          _ => {
            if app.filter_focus {
              app.filter_cursor_up();
            } else {
              app.move_up();
            }
          }
        }
      }
    }
    // ── Left click ─────────────────────────────────────────────────────────
    MouseEventKind::Down(MouseButton::Left) => {
      // Scrollbar track click (feed list jump) — handled before pane focus.
      if mouse.column == scrollbar_col
        && mouse.row >= track_top
        && mouse.row < track_bottom
        && hovered == Some(PaneId::Feed)
      {
        let track_height = (track_bottom - track_top) as usize;
        let click_offset = (mouse.row - track_top) as usize;
        let total = app.visible_items().len();
        if total > 0 && track_height > 0 {
          let new_index =
            ((click_offset * total) / track_height).min(total - 1);
          app.set_active_selected_index(new_index);
        }
        return;
      }

      // Click any open pane → focus it.
      if let Some(pane) = hovered {
        app.focused_pane = pane;
        if matches!(pane, PaneId::Details | PaneId::Feed) {
          app.filter_focus = false;
        }
      }
    }
    _ => {}
  }
}

// ── Background fetch helpers ──────────────────────────────────────────────

pub(crate) fn spawn_fulltext_fetch(
  item: models::FeedItem,
  tx: mpsc::Sender<Result<Vec<String>, String>>,
) {
  std::thread::spawn(move || {
    let _ = tx.send(ingestion::fulltext::fetch(&item));
  });
}

pub(crate) fn spawn_repo_open(
  owner: String,
  repo: String,
  token: String,
  tx: mpsc::Sender<RepoFetchResult>,
) {
  std::thread::spawn(move || {
    let branch = match github::get_default_branch(&owner, &repo, &token) {
      Err(e) => {
        let _ = tx.send(RepoFetchResult::RepoOpened {
          branch: String::new(),
          tree: Err(e),
        });
        return;
      }
      Ok(b) => b,
    };
    let tree = github::fetch_tree_dir(&owner, &repo, &branch, "", &token);
    let _ = tx.send(RepoFetchResult::RepoOpened { branch, tree });
  });
}

pub(crate) fn spawn_repo_dir(
  owner: String,
  repo: String,
  branch: String,
  path: String,
  token: String,
  tx: mpsc::Sender<RepoFetchResult>,
) {
  std::thread::spawn(move || {
    let result = github::fetch_tree_dir(&owner, &repo, &branch, &path, &token);
    let _ = tx.send(RepoFetchResult::DirLoaded { path, result });
  });
}

pub(crate) fn spawn_repo_file(
  owner: String,
  repo: String,
  path: String,
  name: String,
  token: String,
  tx: mpsc::Sender<RepoFetchResult>,
) {
  std::thread::spawn(move || {
    let result = github::fetch_file(&owner, &repo, &path, &token);
    let _ = tx.send(RepoFetchResult::FileLoaded { path, name, result });
  });
}

// ─────────────────────────────────────────────────────────────────────────────

/// `0` = primary pane (Reader if active, else Feed).
/// `1`/`2`/`3` = secondary open panes sorted top-to-bottom, left-to-right.
pub(crate) fn get_pane_by_number(n: u8, app: &App) -> Option<PaneId> {
  match n {
    0 => Some(if app.reader_active { PaneId::Reader } else { PaneId::Feed }),
    1..=3 => app.secondary_panes_sorted().into_iter().nth((n - 1) as usize),
    _ => None,
  }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
  // One-time migration: copy ~/.config/tentative → ~/.config/trench if the
  // old dir exists and the new one does not yet.
  if let Some(home) = dirs::home_dir() {
    let old = home.join(".config/tentative");
    let new = home.join(".config/trench");
    if old.exists() && !new.exists() {
      if let Err(e) = std::fs::rename(&old, &new) {
        eprintln!("trench: could not migrate config dir ({e}); continuing");
      }
    }
  }

  let log_path = dirs::home_dir().unwrap().join(".config/trench/trench.log");
  std::fs::create_dir_all(log_path.parent().unwrap()).unwrap();
  let log_file = std::fs::OpenOptions::new()
    .create(true)
    .append(true)
    .open(&log_path)
    .unwrap();
  env_logger::Builder::new()
    .target(env_logger::Target::Pipe(Box::new(log_file)))
    .filter_level(log::LevelFilter::Debug)
    .init();
  enable_raw_mode()?;
  let mut stdout = io::stdout();
  execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
  let backend = CrosstermBackend::new(stdout);
  let mut terminal = Terminal::new(backend)?;

  let mut app = App::new();

  // Load config.
  let cfg = config::Config::load();
  app.github_token = cfg.github_token.clone();
  app.config = cfg;

  // Load persisted workflow states.
  app.persisted_states = store::load();

  // 1. Load cache immediately → populate app.items.
  let cached = store::cache::load();
  if !cached.is_empty() {
    app.items = cached;
  }

  // 2. Apply persisted states to cached items.
  for item in &mut app.items {
    if let Some(state) = app.persisted_states.get(&item.url) {
      item.workflow_state = state.clone();
    }
  }
  for item in &mut app.discovery_items {
    if let Some(state) = app.persisted_states.get(&item.url) {
      item.workflow_state = state.clone();
    }
  }

  app.list_offset = 0;

  // 4. Spawn background thread to fetch all sources then enrich.
  {
    let (tx, rx) = mpsc::channel::<FetchMessage>();
    app.fetch_rx = Some(rx);
    let mut loading_sources = vec![
      "arxiv".to_string(),
      "huggingface".to_string(),
      "openai".to_string(),
      "deepmind".to_string(),
      "import_ai".to_string(),
      "bair".to_string(),
      "mit_news_ai".to_string(),
      "pwc".to_string(),
      "enriching".to_string(),
    ];
    for feed in &app.config.sources.custom_feeds {
      loading_sources.push(feed.name.clone());
    }
    app.loading_sources = loading_sources;
    app.is_loading = true;
    spawn_fetch(tx, app.config.clone());
  }

  // 3. Start the TUI loop.
  loop {
    // Drain any pending fetch results before drawing.
    app.process_incoming();

    // Tick the embedded reader each frame (voice sync, demo hints, etc.).
    if let Some(editor) = app.reader.as_mut() {
      editor.tick();
    }

    // Tick chat UI each frame (spinner + pending response channel).
    if let Some(chat_ui) = app.chat_ui.as_mut() {
      chat_ui.tick();
    }

    // Tick repo viewer momentum scroll.
    app.repo_tick();

    // ── Drain background fetch results ────────────────────────────────
    if let Some(rx) = app.fulltext_rx.as_ref() {
      let t = std::time::Instant::now();
      match rx.try_recv() {
        Ok(result) => {
          log::debug!(
            "fulltext drain: received result, took {}µs to recv",
            t.elapsed().as_micros()
          );
          app.fulltext_rx = None;
          app.fulltext_loading = false;
          match result {
            Ok(lines) => {
              log::debug!("reader_open: {} lines", lines.len());
              let editor = cli_text_reader::Editor::new(lines, 80);
              app.reader = Some(editor);
              app.reader_active = true;
              app.focused_pane = PaneId::Reader;
              app.clear_notification();
            }
            Err(e) => {
              app.set_notification(format!("Failed to fetch content: {e}"));
            }
          }
        }
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
          log::debug!("fulltext drain: channel disconnected");
          app.fulltext_rx = None;
          app.fulltext_loading = false;
          app.set_notification("Fetch error: thread disconnected".to_string());
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => {}
      }
    }

    if let Some(rx) = app.repo_fetch_rx.as_ref() {
      let t = std::time::Instant::now();
      match rx.try_recv() {
        Ok(result) => {
          log::debug!(
            "repo_fetch drain: received result, took {}µs to recv",
            t.elapsed().as_micros()
          );
          app.repo_fetch_rx = None;
          match result {
            RepoFetchResult::RepoOpened { branch, tree } => match tree {
              Ok(nodes) => {
                if let Some(ctx) = app.repo_context.as_mut() {
                  ctx.default_branch = branch;
                  ctx.tree_nodes = nodes;
                  ctx.status_message = None;
                }
              }
              Err(e) => app.set_repo_status(format!("Error: {e}")),
            },
            RepoFetchResult::DirLoaded { path, result } => {
              app.repo_apply_dir(path, result);
            }
            RepoFetchResult::FileLoaded { path, name, result } => {
              app.repo_apply_file(path, name, result);
            }
          }
        }
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
          log::debug!("repo_fetch drain: channel disconnected");
          app.repo_fetch_rx = None;
          app.set_repo_status("Fetch error: thread disconnected");
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => {}
      }
    }

    let t_draw = std::time::Instant::now();
    terminal.draw(|frame| ui::draw(frame, &mut app))?;
    let draw_ms = t_draw.elapsed().as_millis();
    if draw_ms > 16 {
      log::debug!("terminal.draw took {}ms (slow frame)", draw_ms);
    }

    if event::poll(std::time::Duration::from_millis(16))? {
      match event::read()? {
        Event::Key(key) => {
          if key.kind != KeyEventKind::Press {
            continue;
          }

          log::debug!(
            "key event: {:?} leader_active={} focused_pane={:?}",
            key.code,
            app.leader_active,
            app.focused_pane
          );
          keys::dispatch(key, &mut app);
        }
        Event::Mouse(mouse) => {
          handle_mouse(mouse, &mut app, &terminal);
        }
        _ => {}
      }
    }

    // Drain any stale events that built up during the draw call.
    while event::poll(std::time::Duration::from_millis(0))? {
      let _ = event::read();
    }

    if app.should_quit {
      break;
    }
  }

  // Clean up temp files before restoring the terminal.
  if let Ok(entries) = std::fs::read_dir("/tmp") {
    for entry in entries.flatten() {
      let name = entry.file_name();
      let name_str = name.to_string_lossy();
      if name_str.starts_with("trench_") && name_str.ends_with(".txt") {
        let _ = std::fs::remove_file(entry.path());
      }
    }
  }

  disable_raw_mode()?;
  execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
  Ok(())
}
