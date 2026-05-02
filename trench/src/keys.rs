use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::sync::mpsc;

use crate::app::{
  App, AppView, DiscoverResult, FeedTab, FocusedReader, NavDirection, NotesTab,
  PaneId, RepoContext, RepoPane, SourcesDetectState,
};
use crate::config;
use crate::models::WorkflowState;
use ui_theme::ThemeId;

use super::{
  do_refresh, force_refresh, get_pane_by_number, kbd_scroll_ok, open_url,
  spawn_ai_discovery, spawn_discovery, spawn_fulltext_fetch, spawn_repo_dir,
  spawn_repo_file, spawn_repo_open, truncate_for_notif,
};

/// Top-level key dispatcher — called once per key press event from the main loop.
pub fn dispatch(key: KeyEvent, app: &mut App) {
  // Abstract popup — any of Space / Enter / Esc dismisses.
  if app.abstract_popup_active {
    if matches!(key.code, KeyCode::Char(' ') | KeyCode::Esc | KeyCode::Enter) {
      app.abstract_popup_active = false;
    }
    return;
  }

  // Reader popup (A1) — fully interactive; Esc or reader Quit dismisses.
  if app.reader_popup_active {
    if key.code == KeyCode::Esc {
      app.reader_popup_active = false;
    } else if let Some(editor) = app.reader_popup_editor.as_mut() {
      let action = editor.handle_key(key);
      if matches!(action, cli_text_reader::EditorAction::Quit) {
        app.reader_popup_active = false;
      }
    }
    return;
  }

  // Tab window prompt — intercepts [1]/[2]/Esc to choose which reader pane.
  if app.tab_window_prompt_active {
    match key.code {
      KeyCode::Char('1') => {
        app.tab_window_prompt_active = false;
        app.fulltext_for_secondary = false;
        app.fulltext_new_tab = true;
        trigger_fulltext_new_tab(app);
      }
      KeyCode::Char('2') => {
        app.tab_window_prompt_active = false;
        app.fulltext_for_secondary = true;
        app.fulltext_new_tab = true;
        trigger_fulltext_new_tab(app);
      }
      KeyCode::Esc => {
        app.tab_window_prompt_active = false;
        app.set_notification("Cancelled.".to_string());
      }
      _ => {}
    }
    return;
  }

  // State-3 bottom pane (A2) — handles its own key set when open and focused.
  if app.reader_dual_active && app.reader_bottom_open && app.reader_bottom_focused {
    handle_reader_bottom_pane(key, app);
    return;
  }

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

fn handle_reader_bottom_pane(key: KeyEvent, app: &mut App) {
  match key.code {
    KeyCode::Char('j') | KeyCode::Down => {
      if app.reader_bottom_details {
        app.reader_bottom_scroll = app.reader_bottom_scroll.saturating_add(1);
      } else {
        let count = app.items_for_tab().len();
        if count > 0 {
          app.reader_feed_popup_selected =
            (app.reader_feed_popup_selected + 1).min(count - 1);
        }
      }
    }
    KeyCode::Char('k') | KeyCode::Up => {
      if app.reader_bottom_details {
        app.reader_bottom_scroll = app.reader_bottom_scroll.saturating_sub(1);
      } else {
        app.reader_feed_popup_selected =
          app.reader_feed_popup_selected.saturating_sub(1);
      }
    }
    KeyCode::Char('d') => {
      app.reader_bottom_details = !app.reader_bottom_details;
      app.reader_bottom_scroll = 0;
    }
    KeyCode::Char('/') => {
      app.search_active = true;
      app.search_query.clear();
    }
    KeyCode::Tab => {
      app.feed_tab = match app.feed_tab {
        FeedTab::Inbox => FeedTab::Discoveries,
        FeedTab::Discoveries => FeedTab::Inbox,
      };
      app.reset_active_feed_position();
    }
    KeyCode::Enter => {
      if !app.reader_bottom_details && !app.fulltext_loading {
        let idx = app.reader_feed_popup_selected;
        if let Some(item) = app.items_for_tab().get(idx).cloned() {
          let (tx, rx) = mpsc::channel();
          app.fulltext_rx = Some(rx);
          app.fulltext_loading = true;
          app.fulltext_for_secondary =
            app.focused_reader == FocusedReader::Secondary;
          app.last_read = Some(item.title.clone());
          app.last_read_source = Some(if item.source_name.is_empty() {
            item.source_platform.short_label().to_string()
          } else {
            item.source_name.clone()
          });
          app.set_notification(format!(
            "Fetching: {}…",
            truncate_for_notif(&item.title, 40)
          ));
          spawn_fulltext_fetch(item, tx);
          app.reader_bottom_focused = false;
          app.focused_pane = PaneId::Reader;
        }
      }
    }
    KeyCode::Esc => {
      if app.reader_bottom_details {
        app.reader_bottom_details = false;
        app.reader_bottom_scroll = 0;
      } else {
        app.reader_bottom_open = false;
        app.reader_bottom_focused = false;
        app.focused_pane = PaneId::Reader;
      }
    }
    KeyCode::Char('q') => {
      app.reader_bottom_open = false;
      app.reader_bottom_focused = false;
      app.focused_pane = PaneId::Reader;
    }
    _ => {}
  }
}

// ── Help overlay ─────────────────────────────────────────────────────────────

fn handle_help_overlay(key: KeyEvent, app: &mut App) -> bool {
  if !app.help_active {
    return false;
  }
  match key.code {
    KeyCode::Tab | KeyCode::Char('l') => {
      app.help_section =
        (app.help_section + 1) % crate::ui::HELP_SECTION_COUNT;
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
  let Some(item) = app.selected_item().cloned() else { return; };

  if app.notes_app.is_none() {
    let mut na = notes::app::App::new();
    na.load_state();
    if let Err(e) = na.load_notes() {
      log::error!("notes: failed to load notes: {e}");
    }
    app.notes_app = Some(na);
  }

  // Drop tabs whose note no longer exists (deleted notes, stale ui.json).
  if let Some(na) = app.notes_app.as_ref() {
    app.notes_tabs.retain(|t| na.get_note_title(&t.note_id).is_some());
    app.notes_active_tab = app.notes_active_tab.min(app.notes_tabs.len().saturating_sub(1));
  }

  // Phase 1: find linked notes and collect titles (releases borrow before switch).
  let linked = app.notes_app.as_ref()
    .map(|na| na.find_notes_for_paper(&item.id))
    .unwrap_or_default();

  if linked.is_empty() {
    // No note linked yet — open create popup pre-filled with article title.
    if let Some(na) = app.notes_app.as_mut() {
      if na.focused_note_id().is_none() {
        na.focus_article(&item.id, &item.title, &item.url);
        na.apply_initial_focus();
      }
    }
  } else {
    // Add any linked notes as new tabs (dedup), then activate the first one.
    for note_id in &linked {
      if !app.notes_tabs.iter().any(|t| &t.note_id == note_id) {
        let title = app.notes_app.as_ref()
          .and_then(|na| na.get_note_title(note_id))
          .unwrap_or_default();
        app.notes_tabs.push(NotesTab { note_id: note_id.clone(), title });
      }
    }
    if let Some(idx) = app.notes_tabs.iter().position(|t| linked.contains(&t.note_id)) {
      notes_switch_tab(app, idx);
    }
  }

  app.notes_active = true;
  app.focused_pane = PaneId::Notes;
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
    // A1 — floating reader popup
    KeyCode::Enter => {
      if !app.fulltext_loading && !app.reader_popup_active {
        if let Some(item) = app.selected_item().cloned() {
          let (tx, rx) = mpsc::channel();
          app.reader_popup_rx = Some(rx);
          app.fulltext_loading = true;
          app.last_read = Some(item.title.clone());
          app.last_read_source = Some(if item.source_name.is_empty() {
            item.source_platform.short_label().to_string()
          } else {
            item.source_name.clone()
          });
          app.set_notification(format!(
            "Fetching: {}…",
            truncate_for_notif(&item.title, 40)
          ));
          spawn_fulltext_fetch(item, tx);
        }
      }
    }
    // A2 — three-state split cycle / bottom pane toggle
    KeyCode::Char('v') => {
      if app.reader_dual_active {
        // State 3: toggle bottom feed pane.
        if app.reader_bottom_open {
          app.reader_bottom_open = false;
          app.reader_bottom_focused = false;
          app.reader_bottom_details = false;
          if app.focused_pane == PaneId::Feed {
            app.focused_pane = PaneId::Reader;
          }
        } else {
          app.reader_bottom_open = true;
          app.reader_bottom_focused = true;
          app.reader_bottom_details = false;
        }
      } else if app.reader_split_active {
        // State 2 → State 3: auto-fetch selected item into right pane
        app.reader_dual_active = true;
        app.reader_bottom_focused = false;
        app.reader_bottom_details = false;
        app.reader_bottom_scroll = 0;
        app.fulltext_for_secondary = true;
        if !app.fulltext_loading {
          if let Some(item) = app.selected_item().cloned() {
            let (tx, rx) = mpsc::channel();
            app.fulltext_rx = Some(rx);
            app.fulltext_loading = true;
            app.last_read = Some(item.title.clone());
            app.last_read_source = Some(if item.source_name.is_empty() {
              item.source_platform.short_label().to_string()
            } else {
              item.source_name.clone()
            });
            app.set_notification(format!(
              "Loading: {}…",
              truncate_for_notif(&item.title, 40)
            ));
            spawn_fulltext_fetch(item, tx);
          }
        }
        app.focused_pane = PaneId::Reader;
      } else if app.reader_active {
        // State 1 → State 2: show feed alongside reader
        app.reader_split_active = true;
        app.focused_pane = PaneId::Feed;
      }
    }
    KeyCode::Char('s') => {
      app.sources_cursor = 0;
      app.sources_input.clear();
      app.sources_input_active = false;
      app.view = AppView::Sources;
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
      PaneId::SecondaryReader | PaneId::Reader => {
        close_all_readers(app);
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
    // Ldr+t — open selected item as a new tab
    KeyCode::Char('t') => {
      if app.fulltext_loading {
        return;
      }
      if app.reader_dual_active {
        app.tab_window_prompt_active = true;
        app.set_notification("Add to: [1] left  [2] right  Esc: cancel".to_string());
      } else {
        app.fulltext_new_tab = !app.reader_tabs.is_empty();
        app.fulltext_for_secondary = false;
        trigger_fulltext_new_tab(app);
      }
    }
    // Ldr+[ / Ldr+] — cycle tabs in focused pane
    KeyCode::Char('[') => {
      if app.focused_pane == PaneId::Notes && app.notes_active {
        notes_prev_tab(app);
      } else if app.reader_active {
        app.reader_prev_tab();
      }
    }
    KeyCode::Char(']') => {
      if app.focused_pane == PaneId::Notes && app.notes_active {
        notes_next_tab(app);
      } else if app.reader_active {
        app.reader_next_tab();
      }
    }
    // Ldr+w — close current tab (collapse pane when last tab)
    KeyCode::Char('w') => match app.focused_pane {
      PaneId::Notes if app.notes_active => {
        notes_close_active_tab(app);
      }
      PaneId::SecondaryReader => {
        let pane_empty = app.reader_secondary_close_active_tab();
        if pane_empty {
          app.reader_dual_active = false;
          app.reader_bottom_open = false;
          app.reader_bottom_focused = false;
          app.focused_reader = FocusedReader::Primary;
          app.focused_pane = PaneId::Reader;
        }
      }
      PaneId::Reader if app.reader_active => {
        let pane_empty = app.reader_close_active_tab();
        if pane_empty {
          if app.reader_dual_active {
            app.reader_dual_active = false;
            app.reader_bottom_open = false;
            app.reader_bottom_focused = false;
            app.reader_secondary_tabs.clear();
            app.reader_secondary_active_tab = 0;
          } else if app.reader_split_active {
            app.reader_split_active = false;
          }
          app.focused_pane = PaneId::Feed;
        }
      }
      _ => {}
    },
    _ => {}
  }
}

fn notes_prev_tab(app: &mut App) {
  if app.notes_tabs.is_empty() {
    return;
  }
  let new_idx = app.notes_active_tab.saturating_sub(1);
  notes_switch_tab(app, new_idx);
}

fn notes_next_tab(app: &mut App) {
  if app.notes_tabs.is_empty() {
    return;
  }
  let new_idx = (app.notes_active_tab + 1).min(app.notes_tabs.len() - 1);
  notes_switch_tab(app, new_idx);
}

fn notes_switch_tab(app: &mut App, idx: usize) {
  app.notes_active_tab = idx;
  if let Some(note_id) = app.notes_tabs.get(idx).map(|t| t.note_id.clone()) {
    if let Some(na) = app.notes_app.as_mut() {
      na.focus_note(&note_id);
    }
  }
}

fn notes_close_active_tab(app: &mut App) {
  if app.notes_tabs.is_empty() {
    return;
  }
  // Clamp before remove — stale ui.json can leave active_tab >= len.
  app.notes_active_tab = app.notes_active_tab.min(app.notes_tabs.len() - 1);
  app.notes_tabs.remove(app.notes_active_tab);
  if app.notes_tabs.is_empty() {
    app.notes_active = false;
    app.focused_pane = if app.reader_active { PaneId::Reader } else { PaneId::Feed };
    return;
  }
  app.notes_active_tab = app.notes_active_tab.min(app.notes_tabs.len() - 1);
  notes_switch_tab(app, app.notes_active_tab);
}

/// Spawns a fulltext fetch for the selected item, using flags already set on app.
fn trigger_fulltext_new_tab(app: &mut App) {
  if app.fulltext_loading {
    app.set_notification("Already fetching…".to_string());
    return;
  }
  if let Some(item) = app.selected_item().cloned() {
    let (tx, rx) = mpsc::channel();
    app.fulltext_rx = Some(rx);
    app.fulltext_loading = true;
    app.last_read = Some(item.title.clone());
    app.last_read_source = Some(if item.source_name.is_empty() {
      item.source_platform.short_label().to_string()
    } else {
      item.source_name.clone()
    });
    app.set_notification(format!(
      "Fetching: {}…",
      truncate_for_notif(&item.title, 40)
    ));
    spawn_fulltext_fetch(item, tx);
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
  // Pick up a freshly created note and add its tab.
  if let Some(note_id) = app.notes_app.as_mut().and_then(|na| na.last_created_note_id.take()) {
    let title = app.notes_app.as_ref()
      .and_then(|na| na.get_note_title(&note_id))
      .unwrap_or_default();
    if !app.notes_tabs.iter().any(|t| t.note_id == note_id) {
      app.notes_tabs.push(NotesTab { note_id: note_id.clone(), title });
    }
    if let Some(idx) = app.notes_tabs.iter().position(|t| t.note_id == note_id) {
      app.notes_active_tab = idx;
    }
  }
  true
}

fn handle_reader_pane(key: KeyEvent, app: &mut App) -> bool {
  // Secondary reader (State 3, right pane).
  if app.reader_dual_active && app.focused_pane == PaneId::SecondaryReader {
    log::debug!("routing to secondary reader pane");
    if key.code == KeyCode::Tab {
      if app.reader_bottom_open {
        app.reader_bottom_focused = true;
      }
      return true;
    }
    // Esc in Normal mode: force-close everything and return to feed.
    if key.code == KeyCode::Esc {
      let in_normal = app
        .reader_secondary_editor_mut()
        .map(|e| e.is_normal_mode())
        .unwrap_or(true);
      if in_normal {
        close_all_readers(app);
        return true;
      }
    }
    if let Some(editor) = app.reader_secondary_editor_mut() {
      let action = editor.handle_key(key);
      // q: close the current secondary tab; collapse to primary when empty.
      if matches!(action, cli_text_reader::EditorAction::Quit) {
        let pane_empty = app.reader_secondary_close_active_tab();
        if pane_empty {
          app.reader_dual_active = false;
          app.reader_bottom_open = false;
          app.reader_bottom_focused = false;
          app.focused_reader = FocusedReader::Primary;
          app.focused_pane = PaneId::Reader;
        }
      }
    }
    return true;
  }

  if !(app.reader_active && app.focused_pane == PaneId::Reader) {
    return false;
  }
  log::debug!("routing to reader pane");

  // Tab in primary reader during State 3 → focus secondary reader.
  if app.reader_dual_active && key.code == KeyCode::Tab {
    if !app.reader_secondary_tabs.is_empty() {
      app.focused_pane = PaneId::SecondaryReader;
      app.focused_reader = FocusedReader::Secondary;
    }
    return true;
  }

  // Esc in Normal mode: force-close everything and return to feed.
  if key.code == KeyCode::Esc {
    let in_normal = app
      .reader_editor_mut()
      .map(|e| e.is_normal_mode())
      .unwrap_or(true);
    if in_normal {
      close_all_readers(app);
      return true;
    }
  }

  if let Some(editor) = app.reader_editor_mut() {
    let action = editor.handle_key(key);
    // q: close the current tab; apply state machine only when the pane goes empty.
    if matches!(action, cli_text_reader::EditorAction::Quit) {
      let pane_empty = app.reader_close_active_tab();
      if pane_empty {
        if app.reader_dual_active {
          // Primary ran out of tabs: promote secondary tabs to primary.
          app.reader_tabs =
            std::mem::take(&mut app.reader_secondary_tabs);
          app.reader_active_tab = app.reader_secondary_active_tab;
          app.reader_secondary_active_tab = 0;
          app.reader_active = !app.reader_tabs.is_empty();
          app.reader_dual_active = false;
          app.reader_bottom_open = false;
          app.reader_bottom_focused = false;
          app.focused_reader = FocusedReader::Primary;
          app.focused_pane =
            if app.reader_active { PaneId::Reader } else { PaneId::Feed };
        } else if app.reader_split_active {
          app.reader_split_active = false;
          app.focused_pane = PaneId::Feed;
        } else {
          app.focused_pane = PaneId::Feed;
        }
      }
    }
  }
  true
}

/// Close all reader state and return focus to the feed.
fn close_all_readers(app: &mut App) {
  app.reader_active = false;
  app.reader_dual_active = false;
  app.reader_split_active = false;
  app.reader_bottom_open = false;
  app.reader_bottom_focused = false;
  app.reader_tabs.clear();
  app.reader_active_tab = 0;
  app.reader_secondary_tabs.clear();
  app.reader_secondary_active_tab = 0;
  app.focused_reader = FocusedReader::Primary;
  app.focused_pane = PaneId::Feed;
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
          app.invalidate_visible_cache();
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
  if app.theme_picker_active {
    handle_theme_picker(key, app);
    return true;
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
        app.settings_field = (app.settings_field + 1).min(5);
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
        } else if app.settings_field == 5 {
          open_theme_picker(app);
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
        app.config.theme = app.active_theme;
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

fn open_theme_picker(app: &mut App) {
  let all = ThemeId::all();
  app.theme_picker_active = true;
  app.theme_picker_original = Some(app.active_theme);
  app.theme_picker_cursor =
    all.iter().position(|id| *id == app.active_theme).unwrap_or(0);
  app.theme_picker_scroll = app.theme_picker_cursor.saturating_sub(4);
}

fn handle_theme_picker(key: KeyEvent, app: &mut App) {
  let all = ThemeId::all();
  if all.is_empty() {
    app.theme_picker_active = false;
    return;
  }

  match key.code {
    KeyCode::Char('q') | KeyCode::Esc => {
      if let Some(original) = app.theme_picker_original.take() {
        app.active_theme = original;
      }
      app.theme_picker_active = false;
    }
    KeyCode::Enter => {
      app.active_theme = all[app.theme_picker_cursor.min(all.len() - 1)];
      app.config.theme = app.active_theme;
      app.theme_picker_original = None;
      app.theme_picker_active = false;
    }
    KeyCode::Char('j') | KeyCode::Down => {
      app.theme_picker_cursor = (app.theme_picker_cursor + 1).min(all.len() - 1);
      app.active_theme = all[app.theme_picker_cursor];
      clamp_theme_picker_scroll(app);
    }
    KeyCode::Char('k') | KeyCode::Up => {
      app.theme_picker_cursor = app.theme_picker_cursor.saturating_sub(1);
      app.active_theme = all[app.theme_picker_cursor];
      clamp_theme_picker_scroll(app);
    }
    _ => {}
  }
}

fn clamp_theme_picker_scroll(app: &mut App) {
  const VISIBLE_ROWS: usize = 10;
  if app.theme_picker_cursor < app.theme_picker_scroll {
    app.theme_picker_scroll = app.theme_picker_cursor;
  } else if app.theme_picker_cursor >= app.theme_picker_scroll + VISIBLE_ROWS {
    app.theme_picker_scroll =
      app.theme_picker_cursor.saturating_sub(VISIBLE_ROWS - 1);
  }
}

fn handle_feed_view(key: KeyEvent, app: &mut App) {
  // Discovery query input — highest priority.
  if app.discovery_query_active {
    match key.code {
      KeyCode::Esc => {
        app.discovery_query_active = false;
        app.discovery_query.clear();
      }
      KeyCode::Enter => {
        if !app.discovery_query.is_empty() && !app.discovery_loading {
          let topic = app.discovery_query.clone();
          let config = app.config.clone();
          app.discovery_query_active = false;
          spawn_ai_discovery(topic, config, app);
        }
      }
      KeyCode::Backspace => {
        app.discovery_query.pop();
      }
      KeyCode::Char(c) => {
        app.discovery_query.push(c);
      }
      _ => {}
    }
    return;
  }

  // Discoveries tab — plan checklist keys (when a plan is loaded).
  if app.feed_tab == FeedTab::Discoveries && app.discovery_plan.is_some() {
    match key.code {
      KeyCode::Char('j') | KeyCode::Down => {
        let max = app.discovery_checklist_len().saturating_sub(1);
        app.discovery_plan_cursor = (app.discovery_plan_cursor + 1).min(max);
      }
      KeyCode::Char('k') | KeyCode::Up => {
        app.discovery_plan_cursor = app.discovery_plan_cursor.saturating_sub(1);
      }
      KeyCode::Char(' ') => {
        let idx = app.discovery_plan_cursor;
        app.toggle_plan_selection(idx);
      }
      KeyCode::Char('a') => {
        app.add_selected_plan_sources();
        force_refresh(app);
      }
      KeyCode::Esc => {
        app.discovery_plan = None;
        app.discovery_plan_selected.clear();
        app.discovery_plan_cursor = 0;
      }
      _ => {}
    }
    return;
  }

  // Discoveries tab — no plan: `/` opens query input.
  if app.feed_tab == FeedTab::Discoveries {
    if let KeyCode::Char('/') = key.code {
      app.discovery_query_active = true;
      app.discovery_query.clear();
      return;
    }
  }

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
      KeyCode::Char('f') | KeyCode::Tab => {
        app.filter_focus = false;
      }
      KeyCode::Esc => {
        app.clear_filters();
        app.filter_focus = false;
      }
      _ => {}
    }
  } else if app.focused_pane == PaneId::Feed {
    // In State 2 the narrow feed holds focus — use a restricted key set so
    // main-feed bindings (Esc → quit, v → repo viewer) don't fire here.
    if app.reader_split_active {
      // Close description popup first if open.
      if app.narrow_feed_details_open {
        match key.code {
          KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('d') => {
            app.narrow_feed_details_open = false;
          }
          KeyCode::Char('j') | KeyCode::Down => {
            app.details_scroll = app.details_scroll.saturating_add(1);
          }
          KeyCode::Char('k') | KeyCode::Up => {
            app.details_scroll = app.details_scroll.saturating_sub(1);
          }
          _ => {}
        }
        return;
      }
      match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
          app.reader_split_active = false;
          app.narrow_feed_details_open = false;
          app.focused_pane = PaneId::Reader;
        }
        KeyCode::Char('d') => {
          app.narrow_feed_details_open = true;
          app.details_scroll = 0;
        }
        KeyCode::Char('/') => {
          app.search_active = true;
          app.search_query.clear();
          app.reset_active_feed_position();
        }
        KeyCode::Char('j') | KeyCode::Down => {
          if kbd_scroll_ok(app) {
            app.move_down();
            app.narrow_feed_details_open = false;
          }
        }
        KeyCode::Char('k') | KeyCode::Up => {
          if kbd_scroll_ok(app) {
            app.move_up();
            app.narrow_feed_details_open = false;
          }
        }
        KeyCode::Enter => {
          if !app.fulltext_loading {
            if let Some(item) = app.selected_item().cloned() {
              let (tx, rx) = mpsc::channel();
              app.fulltext_rx = Some(rx);
              app.fulltext_loading = true;
              app.fulltext_for_secondary = false;
              app.narrow_feed_details_open = false;
              app.last_read = Some(item.title.clone());
              app.last_read_source = Some(if item.source_name.is_empty() {
                item.source_platform.short_label().to_string()
              } else {
                item.source_name.clone()
              });
              app.set_notification(format!(
                "Fetching: {}…",
                truncate_for_notif(&item.title, 40)
              ));
              app.focused_pane = PaneId::Reader;
              spawn_fulltext_fetch(item, tx);
            }
          }
        }
        _ => {}
      }
      return;
    }

    match key.code {
      KeyCode::Tab => {
        app.feed_tab = match app.feed_tab {
          FeedTab::Inbox => FeedTab::Discoveries,
          FeedTab::Discoveries => FeedTab::Inbox,
        };
        app.reset_active_feed_position();
      }
      KeyCode::Char('f') => {
        app.filter_focus = true;
      }
      KeyCode::Esc => app.should_quit = true,
      KeyCode::Char('l') | KeyCode::Right => {}
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
      KeyCode::Char(' ') => {
        if app.selected_item().is_some() {
          app.abstract_popup_active = true;
        }
      }
      KeyCode::Enter => {
        log::debug!(
          "feed Enter: fulltext_loading={} block_reader_loading={}",
          app.fulltext_loading,
          app.block_reader_loading
        );
        if !app.fulltext_loading && !app.block_reader_loading {
          if let Some(item) = app.selected_item().cloned() {
            app.last_read = Some(item.title.clone());
            app.last_read_source = Some(if item.source_name.is_empty() {
              item.source_platform.short_label().to_string()
            } else {
              item.source_name.clone()
            });
            // TODO: switch back to block_reader for arXiv once it's ready.
            {
              log::debug!(
                "feed Enter: spawning fulltext fetch for url={}",
                item.url
              );
              let t = std::time::Instant::now();
              app.fulltext_loading = true;
              app.set_notification(format!(
                "Fetching: {}…",
                truncate_for_notif(&item.title, 40)
              ));
              let (tx, rx) = mpsc::channel();
              app.fulltext_rx = Some(rx);
              spawn_fulltext_fetch(item, tx);
              log::debug!(
                "feed Enter: fetch setup took {}µs",
                t.elapsed().as_micros()
              );
            }
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
