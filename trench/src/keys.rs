use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::sync::mpsc;

use crate::app::{
  App, AppView, DiscoverResult, FeedTab, NavDirection, PaneId, RepoContext,
  RepoPane, SourcesDetectState,
};
use crate::config;
use crate::models::WorkflowState;

use super::{
  do_refresh, force_refresh, get_pane_by_number, kbd_scroll_ok, open_url,
  spawn_discovery, spawn_fulltext_fetch, spawn_repo_dir, spawn_repo_file,
  spawn_repo_open, truncate_for_notif,
};

/// Top-level key dispatcher — called once per key press event from the main loop.
pub fn dispatch(key: KeyEvent, app: &mut App) {
  if handle_help_overlay(key, app) {
    return;
  }
  if handle_leader_or_ctrl_t(key, app) {
    return;
  }
  if handle_chat_pane(key, app) {
    return;
  }
  if handle_notes_pane(key, app) {
    return;
  }
  if handle_reader_pane(key, app) {
    return;
  }
  if handle_repo_viewer(key, app) {
    return;
  }
  if handle_sources_popup(key, app) {
    return;
  }
  if handle_settings_view(key, app) {
    return;
  }
  handle_feed_view(key, app);
}

// ── Help overlay ─────────────────────────────────────────────────────────────

fn handle_help_overlay(key: KeyEvent, app: &mut App) -> bool {
  if !app.help_active {
    return false;
  }
  match key.code {
    KeyCode::Tab | KeyCode::Char('l') => {
      app.help_section = (app.help_section + 1) % 6;
      app.help_scroll = 0;
    }
    KeyCode::BackTab | KeyCode::Char('h') => {
      app.help_section = app.help_section.saturating_sub(1);
      app.help_scroll = 0;
    }
    KeyCode::Char('j') | KeyCode::Down => {
      app.help_scroll = app.help_scroll.saturating_add(1);
    }
    KeyCode::Char('k') | KeyCode::Up => {
      app.help_scroll = app.help_scroll.saturating_sub(1);
    }
    KeyCode::Char('q') | KeyCode::Esc => {
      app.help_active = false;
    }
    _ => {}
  }
  true
}

// ── Leader key (Ctrl+T) ───────────────────────────────────────────────────────

fn handle_leader_or_ctrl_t(key: KeyEvent, app: &mut App) -> bool {
  // Expire leader if timeout elapsed.
  if app.leader_active
    && app
      .leader_activated_at
      .map(|t| t.elapsed().as_millis() > app.leader_timeout_ms as u128)
      .unwrap_or(false)
  {
    app.leader_active = false;
  }

  // Ctrl+T: arm the leader.
  if key.code == KeyCode::Char('t') && key.modifiers == KeyModifiers::CONTROL {
    app.leader_active = true;
    app.leader_activated_at = Some(std::time::Instant::now());
    return true;
  }

  if !app.leader_active {
    return false;
  }
  handle_leader(key, app);
  true
}

fn open_notes(app: &mut App) {
  if let Some(item) = app.selected_item().cloned() {
    if app.notes_app.is_none() {
      let mut na = notes::app::App::new();
      na.load_state();
      if let Err(e) = na.load_notes() {
        log::error!("notes: failed to load notes: {e}");
      }
      app.notes_app = Some(na);
    }
    if let Some(na) = app.notes_app.as_mut() {
      na.focus_article(&item.id, &item.title, &item.url);
      na.apply_initial_focus();
    }
    app.notes_active = true;
    app.focused_pane = PaneId::Notes;
  }
}

fn handle_leader(key: KeyEvent, app: &mut App) {
  log::debug!("leader dispatch: {:?}", key.code);
  let is_nav = matches!(
    key.code,
    KeyCode::Char('h')
      | KeyCode::Char('j')
      | KeyCode::Char('k')
      | KeyCode::Char('l')
  );
  if !is_nav {
    app.leader_active = false;
  }

  match key.code {
    KeyCode::Char('n') => {
      if app.notes_active {
        app.notes_active = false;
        app.focused_pane =
          if app.reader_active { PaneId::Reader } else { PaneId::Feed };
      } else {
        open_notes(app);
      }
    }
    KeyCode::Char('c') => {
      if app.chat_active {
        app.chat_active = false;
        app.chat_fullscreen = false;
        app.focused_pane =
          if app.reader_active { PaneId::Reader } else { PaneId::Feed };
      } else {
        if app.chat_ui.is_none() {
          let mut registry = chat::ProviderRegistry::new();
          if let Some(k) =
            app.config.claude_api_key.as_ref().filter(|k| !k.is_empty())
          {
            registry.register(
              "claude",
              Box::new(chat::ClaudeProvider::new(k.clone())),
            );
          }
          if let Some(k) =
            app.config.openai_api_key.as_ref().filter(|k| !k.is_empty())
          {
            registry.register(
              "openai",
              Box::new(chat::OpenAiProvider::new(k.clone())),
            );
          }
          let default_provider = app.config.default_chat_provider.clone();
          let slash_commands = crate::commands::registry::chat_slash_specs();
          app.chat_ui =
            Some(chat::ChatUi::new(registry, default_provider, slash_commands));
        }
        app.notes_active = false;
        app.chat_active = true;
        app.focused_pane = PaneId::Chat;
      }
    }
    KeyCode::Char('S') => {
      app.settings_github_token =
        app.config.github_token.clone().unwrap_or_default();
      app.settings_s2_key =
        app.config.semantic_scholar_key.clone().unwrap_or_default();
      app.settings_claude_key =
        app.config.claude_api_key.clone().unwrap_or_default();
      app.settings_openai_key =
        app.config.openai_api_key.clone().unwrap_or_default();
      app.settings_default_chat_provider =
        app.config.default_chat_provider.clone();
      app.settings_field = 0;
      app.settings_editing = false;
      app.view = AppView::Settings;
    }
    KeyCode::Char('z') => {
      if app.chat_active {
        app.chat_at_top = !app.chat_at_top;
      }
    }
    KeyCode::Char('d') => {
      app.feed_tab = match app.feed_tab {
        FeedTab::Inbox => FeedTab::Discoveries,
        FeedTab::Discoveries => FeedTab::Inbox,
      };
      app.reset_active_feed_position();
      app.focused_pane = PaneId::Feed;
    }
    KeyCode::Char('?') => {
      app.help_active = true;
      app.help_section = 0;
      app.help_scroll = 0;
    }
    KeyCode::Char('q') => {
      app.should_quit = true;
    }
    KeyCode::Char('h') => {
      let t = std::time::Instant::now();
      let result = app.find_pane_in_direction(NavDirection::Left);
      log::debug!(
        "find_pane Left={:?} took {}µs",
        result,
        t.elapsed().as_micros()
      );
      if let Some(pane) = result {
        app.focused_pane = pane;
      }
    }
    KeyCode::Char('j') => {
      let t = std::time::Instant::now();
      let result = app.find_pane_in_direction(NavDirection::Down);
      log::debug!(
        "find_pane Down={:?} took {}µs",
        result,
        t.elapsed().as_micros()
      );
      if let Some(pane) = result {
        app.focused_pane = pane;
      }
    }
    KeyCode::Char('k') => {
      let t = std::time::Instant::now();
      let result = app.find_pane_in_direction(NavDirection::Up);
      log::debug!(
        "find_pane Up={:?} took {}µs",
        result,
        t.elapsed().as_micros()
      );
      if let Some(pane) = result {
        app.focused_pane = pane;
      }
    }
    KeyCode::Char('l') => {
      let t = std::time::Instant::now();
      let result = app.find_pane_in_direction(NavDirection::Right);
      log::debug!(
        "find_pane Right={:?} took {}µs",
        result,
        t.elapsed().as_micros()
      );
      if let Some(pane) = result {
        app.focused_pane = pane;
      }
    }
    KeyCode::Esc => match app.focused_pane {
      PaneId::Chat => {
        app.chat_active = false;
        app.chat_fullscreen = false;
        app.focused_pane =
          if app.reader_active { PaneId::Reader } else { PaneId::Feed };
      }
      PaneId::Notes => {
        if let Some(na) = app.notes_app.as_mut() {
          let _ = na.persist_state();
        }
        app.notes_active = false;
        app.focused_pane =
          if app.reader_active { PaneId::Reader } else { PaneId::Feed };
      }
      PaneId::Reader => {
        app.reader_active = false;
        app.reader = None;
        app.focused_pane = PaneId::Feed;
      }
      PaneId::Feed | PaneId::Details => {}
    },
    KeyCode::Char('0') => {
      if let Some(pane) = get_pane_by_number(0, app) {
        app.focused_pane = pane;
      }
    }
    KeyCode::Char('1') => {
      if let Some(pane) = get_pane_by_number(1, app) {
        app.focused_pane = pane;
      }
    }
    KeyCode::Char('2') => {
      if let Some(pane) = get_pane_by_number(2, app) {
        app.focused_pane = pane;
      }
    }
    KeyCode::Char('3') => {
      if let Some(pane) = get_pane_by_number(3, app) {
        app.focused_pane = pane;
      }
    }
    _ => {}
  }
}

// ── Pane routers ─────────────────────────────────────────────────────────────

fn handle_chat_pane(key: KeyEvent, app: &mut App) -> bool {
  if !(app.chat_active && app.focused_pane == PaneId::Chat) {
    return false;
  }
  log::debug!("routing to chat pane");
  if let Some(chat_ui) = app.chat_ui.as_mut() {
    let action = chat_ui.handle_key(key);
    match action {
      chat::ChatAction::Quit => {
        app.chat_active = false;
        app.chat_fullscreen = false;
        app.focused_pane =
          if app.reader_active { PaneId::Reader } else { PaneId::Feed };
      }
      chat::ChatAction::SlashCommand(cmd) => {
        app.handle_slash_command(cmd);
      }
      chat::ChatAction::None | chat::ChatAction::Sending => {}
    }
  }
  true
}

fn handle_notes_pane(key: KeyEvent, app: &mut App) -> bool {
  if !(app.notes_active && app.focused_pane == PaneId::Notes) {
    return false;
  }
  log::debug!("routing to notes pane");
  if let Some(notes_app) = app.notes_app.as_mut() {
    if notes::handle_key(key, notes_app) {
      if let Err(e) = notes_app.persist_state() {
        log::error!("notes: failed to persist state: {e}");
      }
      app.notes_active = false;
      app.focused_pane =
        if app.reader_active { PaneId::Reader } else { PaneId::Feed };
    }
  }
  true
}

fn handle_reader_pane(key: KeyEvent, app: &mut App) -> bool {
  if !(app.reader_active && app.focused_pane == PaneId::Reader) {
    return false;
  }
  log::debug!("routing to reader pane");
  if let Some(editor) = app.reader.as_mut() {
    let action = editor.handle_key(key);
    if matches!(action, cli_text_reader::EditorAction::Quit) {
      app.reader_active = false;
      app.reader = None;
      app.focused_pane = PaneId::Feed;
    }
  }
  true
}

// ── View handlers ─────────────────────────────────────────────────────────────

fn handle_repo_viewer(key: KeyEvent, app: &mut App) -> bool {
  if app.view != AppView::RepoViewer {
    return false;
  }
  log::debug!("routing to repo viewer");
  match key.code {
    KeyCode::Char('q') => app.close_repo_viewer(),
    KeyCode::Tab => app.repo_switch_pane(),
    KeyCode::Char('j') | KeyCode::Down => app.repo_nav_down(0),
    KeyCode::Char('k') | KeyCode::Up => app.repo_nav_up(),
    KeyCode::Char('h') | KeyCode::Left => app.repo_pan_left(),
    KeyCode::Char('l') | KeyCode::Right => app.repo_pan_right(),
    KeyCode::Char('+') | KeyCode::Char('=') => app.repo_zoom_in(),
    KeyCode::Char('-') => app.repo_zoom_out(),
    KeyCode::Char('y') => app.repo_copy_path(),
    KeyCode::Char('d') => app.repo_download_file(),
    KeyCode::Enter => {
      log::debug!(
        "repo Enter: repo_fetch_rx active={}",
        app.repo_fetch_rx.is_some()
      );
      if app.repo_fetch_rx.is_none() {
        if let Some(target) = app.repo_enter_target() {
          let token = app.github_token.clone().unwrap_or_default();
          match target {
            crate::app::RepoEnterTarget::Dir(path) => {
              if let Some(ctx) = &app.repo_context {
                let (owner, repo, branch) = (
                  ctx.owner.clone(),
                  ctx.repo_name.clone(),
                  ctx.default_branch.clone(),
                );
                log::debug!("repo Enter: spawning dir fetch path={:?}", path);
                app.set_repo_status("Loading…");
                let (tx, rx) = mpsc::channel();
                app.repo_fetch_rx = Some(rx);
                spawn_repo_dir(owner, repo, branch, path, token, tx);
              }
            }
            crate::app::RepoEnterTarget::File(path, name) => {
              if let Some(ctx) = &app.repo_context {
                let (owner, repo) = (ctx.owner.clone(), ctx.repo_name.clone());
                log::debug!("repo Enter: spawning file fetch path={:?}", path);
                app.set_repo_status("Loading…");
                let (tx, rx) = mpsc::channel();
                app.repo_fetch_rx = Some(rx);
                spawn_repo_file(owner, repo, path, name, token, tx);
              }
            }
          }
        }
      }
    }
    KeyCode::Char('b') | KeyCode::Backspace => {
      if app.repo_back_target().is_none()
        && app.repo_context.as_ref().is_some_and(|c| !c.no_token)
      {
        app.set_repo_status("Already at root");
      } else if let Some(parent) = app.repo_back_target() {
        if app.repo_fetch_rx.is_none() {
          if let Some(ctx) = &app.repo_context {
            let (owner, repo, branch) = (
              ctx.owner.clone(),
              ctx.repo_name.clone(),
              ctx.default_branch.clone(),
            );
            let token = app.github_token.clone().unwrap_or_default();
            app.set_repo_status("Loading…");
            let (tx, rx) = mpsc::channel();
            app.repo_fetch_rx = Some(rx);
            spawn_repo_dir(owner, repo, branch, parent, token, tx);
          }
        }
      }
    }
    _ => {}
  }
  true
}

fn handle_sources_popup(key: KeyEvent, app: &mut App) -> bool {
  if app.view != AppView::Sources {
    return false;
  }
  if app.sources_input_active {
    match key.code {
      KeyCode::Esc => {
        app.sources_input_active = false;
        app.sources_detect_state = SourcesDetectState::Idle;
        app.sources_input.clear();
      }
      KeyCode::Enter => {
        match &app.sources_detect_state {
          SourcesDetectState::Idle => {
            if !app.sources_input.is_empty() {
              app.sources_detect_state = SourcesDetectState::Detecting;
              let url = app.sources_input.clone();
              let (dtx, drx) = mpsc::channel();
              app.sources_detect_rx = Some(drx);
              spawn_discovery(url, dtx);
            }
          }
          SourcesDetectState::Detecting => {
            // waiting — do nothing
          }
          SourcesDetectState::Result(result) => {
            let result = result.clone();
            match &result {
              DiscoverResult::ArxivCategory(code) => {
                if !app.config.sources.arxiv_categories.contains(code) {
                  log::debug!(
                    "sources_popup: adding arxiv category via detection: {code}"
                  );
                  app.config.sources.arxiv_categories.push(code.clone());
                  app.config.save();
                  log::debug!(
                    "sources_popup: saved — arxiv categories now: [{}]",
                    app.config.sources.arxiv_categories.join(", ")
                  );
                  force_refresh(app);
                }
              }
              DiscoverResult::RssFeed { url, name } => {
                let exists =
                  app.config.sources.custom_feeds.iter().any(|f| &f.url == url);
                if !exists {
                  app.config.sources.custom_feeds.push(config::CustomFeed {
                    url: url.clone(),
                    name: name.clone(),
                    feed_type: "rss".to_string(),
                  });
                  app.config.save();
                  force_refresh(app);
                }
              }
              DiscoverResult::HuggingFaceAlreadyEnabled
              | DiscoverResult::Failed(_) => {}
            }
            app.sources_input.clear();
            app.sources_detect_state = SourcesDetectState::Idle;
            app.sources_input_active = false;
          }
        }
      }
      KeyCode::Backspace => {
        app.sources_input.pop();
        app.sources_detect_state = SourcesDetectState::Idle;
      }
      KeyCode::Char(c) => {
        app.sources_input.push(c);
        app.sources_detect_state = SourcesDetectState::Idle;
      }
      _ => {}
    }
  } else {
    let cats = app.sources_popup_arxiv_cats();
    let cats_count = cats.len();
    let sources_count = config::PREDEFINED_SOURCES.len();
    let custom_count = app.config.sources.custom_feeds.len();
    let total = app.sources_popup_total_items();

    match key.code {
      KeyCode::Esc | KeyCode::Char('q') => {
        app.view = AppView::Settings;
        app.sources_cursor = 0;
        app.sources_input.clear();
        app.sources_detect_state = SourcesDetectState::Idle;
      }
      KeyCode::Char('j') | KeyCode::Down => {
        app.sources_cursor =
          (app.sources_cursor + 1).min(total.saturating_sub(1));
      }
      KeyCode::Char('k') | KeyCode::Up => {
        app.sources_cursor = app.sources_cursor.saturating_sub(1);
      }
      KeyCode::Enter | KeyCode::Char('/') => {
        if app.sources_cursor == 0 {
          app.sources_input_active = true;
        }
      }
      KeyCode::Char(' ') => {
        let c = app.sources_cursor;
        if c == 0 {
          app.sources_input_active = true;
        } else if c <= cats_count {
          let code = cats[c - 1].0.clone();
          if app.config.sources.arxiv_categories.contains(&code) {
            log::debug!("sources_popup: removing arxiv category: {code}");
            app.config.sources.arxiv_categories.retain(|x| x != &code);
          } else {
            log::debug!("sources_popup: adding arxiv category: {code}");
            app.config.sources.arxiv_categories.push(code);
          }
          app.config.save();
          log::debug!(
            "sources_popup: saved — arxiv categories now: [{}]",
            app.config.sources.arxiv_categories.join(", ")
          );
          force_refresh(app);
        } else if c <= cats_count + sources_count {
          let src = config::PREDEFINED_SOURCES[c - cats_count - 1];
          let cur = app
            .config
            .sources
            .enabled_sources
            .get(src)
            .copied()
            .unwrap_or(true);
          app.config.sources.enabled_sources.insert(src.to_string(), !cur);
          app.config.save();
          force_refresh(app);
        }
        // custom feeds: no toggle (present = enabled, use d to delete)
      }
      KeyCode::Char('d') => {
        let c = app.sources_cursor;
        let custom_start = 1 + cats_count + sources_count;
        if c >= custom_start && c < custom_start + custom_count {
          let idx = c - custom_start;
          app.config.sources.custom_feeds.remove(idx);
          app.config.save();
          app.sources_cursor = app.sources_cursor.saturating_sub(1);
        }
      }
      _ => {}
    }
  }
  true
}

fn handle_settings_view(key: KeyEvent, app: &mut App) -> bool {
  if app.view != AppView::Settings {
    return false;
  }
  if app.settings_editing {
    match key.code {
      KeyCode::Enter => {
        match app.settings_field {
          0 => app.settings_github_token = app.settings_edit_buf.clone(),
          1 => app.settings_s2_key = app.settings_edit_buf.clone(),
          2 => app.settings_claude_key = app.settings_edit_buf.clone(),
          3 => app.settings_openai_key = app.settings_edit_buf.clone(),
          _ => {}
        }
        app.settings_editing = false;
      }
      KeyCode::Esc => {
        app.settings_editing = false;
        app.settings_edit_buf.clear();
      }
      KeyCode::Backspace => {
        app.settings_edit_buf.pop();
      }
      KeyCode::Char(c) => {
        app.settings_edit_buf.push(c);
      }
      _ => {}
    }
  } else {
    match key.code {
      KeyCode::Char('q') | KeyCode::Esc => {
        app.view = AppView::Feed;
      }
      KeyCode::Char('j') | KeyCode::Down => {
        app.settings_field = (app.settings_field + 1).min(4);
      }
      KeyCode::Char('k') | KeyCode::Up => {
        app.settings_field = app.settings_field.saturating_sub(1);
      }
      KeyCode::Enter => {
        if app.settings_field == 4 {
          app.settings_default_chat_provider =
            if app.settings_default_chat_provider == "claude" {
              "openai".to_string()
            } else {
              "claude".to_string()
            };
        } else {
          app.settings_edit_buf = match app.settings_field {
            0 => app.settings_github_token.clone(),
            1 => app.settings_s2_key.clone(),
            2 => app.settings_claude_key.clone(),
            3 => app.settings_openai_key.clone(),
            _ => String::new(),
          };
          app.settings_editing = true;
        }
      }
      KeyCode::Char('s') | KeyCode::Char('S') => {
        app.config.github_token = if app.settings_github_token.is_empty() {
          None
        } else {
          Some(app.settings_github_token.clone())
        };
        app.config.semantic_scholar_key = if app.settings_s2_key.is_empty() {
          None
        } else {
          Some(app.settings_s2_key.clone())
        };
        app.config.claude_api_key = if app.settings_claude_key.is_empty() {
          None
        } else {
          Some(app.settings_claude_key.clone())
        };
        app.config.openai_api_key = if app.settings_openai_key.is_empty() {
          None
        } else {
          Some(app.settings_openai_key.clone())
        };
        app.config.default_chat_provider =
          app.settings_default_chat_provider.clone();
        // Keep github_token field in sync for repo viewer.
        app.github_token = app.config.github_token.clone();
        // Rebuild chat_ui with updated keys on next open.
        app.chat_ui = None;
        app.config.save();
        app.settings_save_time = Some(std::time::Instant::now());
      }
      KeyCode::Char('p') => {
        app.sources_cursor = 0;
        app.sources_input.clear();
        app.sources_input_active = false;
        app.sources_detect_state = SourcesDetectState::Idle;
        app.view = AppView::Sources;
        log::debug!(
          "sources_popup: opened — current arxiv categories: [{}]",
          app.config.sources.arxiv_categories.join(", ")
        );
      }
      _ => {}
    }
  }
  true
}

fn handle_feed_view(key: KeyEvent, app: &mut App) {
  if app.search_active {
    match key.code {
      KeyCode::Esc => {
        app.search_active = false;
        app.search_query.clear();
        app.reset_active_feed_position();
      }
      KeyCode::Enter => {
        app.search_active = false;
      }
      KeyCode::Backspace => {
        app.pop_search_char();
      }
      KeyCode::Char(c) => {
        app.push_search_char(c);
      }
      _ => {}
    }
  } else if app.filter_focus {
    match key.code {
      KeyCode::Char('j') | KeyCode::Down => {
        if kbd_scroll_ok(app) {
          app.filter_cursor_down();
        }
      }
      KeyCode::Char('k') | KeyCode::Up => {
        if kbd_scroll_ok(app) {
          app.filter_cursor_up();
        }
      }
      KeyCode::Char(' ') => app.toggle_filter_at_cursor(),
      KeyCode::Char('c') => app.clear_filters(),
      KeyCode::Tab => {
        app.filter_focus = false;
      }
      KeyCode::Esc => {
        app.clear_filters();
        app.filter_focus = false;
      }
      _ => {}
    }
  } else if app.focused_pane == PaneId::Details {
    match key.code {
      KeyCode::Char('j') | KeyCode::Down => {
        if kbd_scroll_ok(app) {
          app.details_scroll_down();
        }
      }
      KeyCode::Char('k') | KeyCode::Up => {
        if kbd_scroll_ok(app) {
          app.details_scroll_up();
        }
      }
      KeyCode::PageDown => {
        if kbd_scroll_ok(app) {
          app.details_scroll = app.details_scroll.saturating_add(10);
        }
      }
      KeyCode::PageUp => {
        if kbd_scroll_ok(app) {
          app.details_scroll = app.details_scroll.saturating_sub(10);
        }
      }
      KeyCode::Char('g') => app.details_scroll = 0,
      KeyCode::Char('G') => app.details_scroll = usize::MAX,
      _ => {}
    }
  } else if app.focused_pane == PaneId::Feed {
    match key.code {
      KeyCode::Tab => {
        app.filter_focus = true;
      }
      KeyCode::Esc => app.should_quit = true,
      KeyCode::Char('l') | KeyCode::Right => {
        app.focused_pane = PaneId::Details;
      }
      KeyCode::Char('h') | KeyCode::Left => {
        // already on Feed — no-op
      }
      KeyCode::Char('j') | KeyCode::Down => {
        if kbd_scroll_ok(app) {
          app.move_down();
        }
      }
      KeyCode::Char('k') | KeyCode::Up => {
        if kbd_scroll_ok(app) {
          app.move_up();
        }
      }
      KeyCode::PageDown | KeyCode::PageUp => {}
      KeyCode::Char('g') => {
        app.go_to_top();
      }
      KeyCode::Char('G') => {
        app.go_to_bottom();
      }
      KeyCode::Enter => {
        log::debug!("feed Enter: fulltext_loading={}", app.fulltext_loading);
        if !app.fulltext_loading {
          if let Some(item) = app.selected_item().cloned() {
            log::debug!(
              "feed Enter: spawning fulltext fetch for url={}",
              item.url
            );
            let t = std::time::Instant::now();
            let (tx, rx) = mpsc::channel();
            app.fulltext_rx = Some(rx);
            app.fulltext_loading = true;
            app.set_notification(format!(
              "Fetching: {}…",
              truncate_for_notif(&item.title, 40)
            ));
            spawn_fulltext_fetch(item, tx);
            log::debug!(
              "feed Enter: spawn_fulltext_fetch setup took {}µs",
              t.elapsed().as_micros()
            );
          }
        }
      }
      KeyCode::Char('/') => {
        app.search_active = true;
        app.search_query.clear();
        app.reset_active_feed_position();
      }
      KeyCode::Char('i') => app.set_workflow_state(WorkflowState::Inbox),
      KeyCode::Char('s') => app.set_workflow_state(WorkflowState::Skimmed),
      KeyCode::Char('r') => app.set_workflow_state(WorkflowState::DeepRead),
      KeyCode::Char('w') => app.set_workflow_state(WorkflowState::Queued),
      KeyCode::Char('x') => app.set_workflow_state(WorkflowState::Archived),
      KeyCode::Char('o') => {
        if let Some(item) = app.selected_item() {
          let url = item.url.clone();
          let title = truncate_for_notif(&item.title, 40);
          open_url(&url);
          app.set_notification(format!("Opened in browser: {title}"));
        }
      }
      KeyCode::Char('R') => {
        if app.is_loading || app.is_refreshing {
          app.set_notification("Already refreshing...".to_string());
        } else {
          app.clear_notification();
          do_refresh(app);
        }
      }
      KeyCode::Char('v') => {
        if let Some(item) = app.selected_item() {
          if item.github_owner.is_none() || item.github_repo_name.is_none() {
            app.status_message = Some("No repo linked".to_string());
          } else if let (Some(owner), Some(repo_name)) =
            (item.github_owner.clone(), item.github_repo_name.clone())
          {
            let token = app.github_token.clone().unwrap_or_default();
            if token.is_empty() {
              app.repo_context = Some(RepoContext {
                owner,
                repo_name,
                default_branch: String::new(),
                tree_path: String::new(),
                tree_nodes: Vec::new(),
                tree_cursor: 0,
                file_path: None,
                file_name: None,
                raw_file_content: String::new(),
                file_kind: crate::app::RepoFileKind::PlainText,
                file_lines: Vec::new(),
                file_highlighted: Vec::new(),
                markdown_cache: None,
                rendered_line_count: 0,
                markdown_has_pannable_lines: false,
                file_scroll: 0,
                pane_focus: RepoPane::Tree,
                status_message: None,
                no_token: true,
                h_offset: 0,
                wrap_width: 0,
                scroll_velocity: 0.0,
              });
              app.view = AppView::RepoViewer;
            } else {
              app.repo_context = Some(RepoContext {
                owner: owner.clone(),
                repo_name: repo_name.clone(),
                default_branch: String::new(),
                tree_path: String::new(),
                tree_nodes: Vec::new(),
                tree_cursor: 0,
                file_path: None,
                file_name: None,
                raw_file_content: String::new(),
                file_kind: crate::app::RepoFileKind::PlainText,
                file_lines: Vec::new(),
                file_highlighted: Vec::new(),
                markdown_cache: None,
                rendered_line_count: 0,
                markdown_has_pannable_lines: false,
                file_scroll: 0,
                pane_focus: RepoPane::Tree,
                status_message: Some("Loading…".into()),
                no_token: false,
                h_offset: 0,
                wrap_width: 0,
                scroll_velocity: 0.0,
              });
              app.view = AppView::RepoViewer;
              let (tx, rx) = mpsc::channel();
              app.repo_fetch_rx = Some(rx);
              spawn_repo_open(owner, repo_name, token, tx);
            }
          }
        }
      }
      _ => {}
    }
  }
}
