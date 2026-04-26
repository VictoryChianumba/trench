use crate::config::Config;
use crate::discovery::{DiscoveryMessage, DiscoveryPlan};
use crate::ingestion::message::FetchMessage;
use crate::models::*;
use chrono::Utc;
use ratatui::layout::Rect;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Receiver;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Filter state
// ---------------------------------------------------------------------------

pub struct FilterState {
  pub sources: HashSet<String>,
  pub signals: HashSet<SignalLevel>,
  pub content_types: HashSet<ContentType>,
  pub workflow_states: HashSet<WorkflowState>,
}

impl Default for FilterState {
  fn default() -> Self {
    Self {
      sources: HashSet::new(),
      signals: HashSet::new(),
      content_types: HashSet::new(),
      workflow_states: HashSet::new(),
    }
  }
}

impl FilterState {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn is_empty(&self) -> bool {
    self.sources.is_empty()
      && self.signals.is_empty()
      && self.content_types.is_empty()
      && self.workflow_states.is_empty()
  }

  pub fn matches(&self, item: &FeedItem) -> bool {
    (self.sources.is_empty() || {
      let sname = if item.source_name.is_empty() {
        item.source_platform.short_label().to_string()
      } else {
        item.source_name.clone()
      };
      self.sources.contains(&sname)
    }) && (self.signals.is_empty() || self.signals.contains(&item.signal))
      && (self.content_types.is_empty()
        || self.content_types.contains(&item.content_type))
      && (self.workflow_states.is_empty()
        || self.workflow_states.contains(&item.workflow_state))
  }

  pub fn active_count(&self) -> usize {
    self.sources.len()
      + self.signals.len()
      + self.content_types.len()
      + self.workflow_states.len()
  }
}

// ---------------------------------------------------------------------------
// Repo viewer types
// ---------------------------------------------------------------------------

#[derive(PartialEq)]
pub enum RepoPane {
  Tree,
  File,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepoFileKind {
  Markdown,
  Code,
  PlainText,
}

pub struct RepoContext {
  pub owner: String,
  pub repo_name: String,
  pub default_branch: String,
  pub tree_path: String,
  pub tree_nodes: Vec<crate::github::TreeNode>,
  pub tree_cursor: usize,
  pub file_path: Option<String>,
  pub file_name: Option<String>,
  pub raw_file_content: String,
  pub file_kind: RepoFileKind,
  pub file_lines: Vec<String>,
  pub file_highlighted: Vec<Vec<(u8, u8, u8, String)>>,
  pub markdown_cache: Option<crate::ui::repo_markdown::MarkdownRenderCache>,
  pub rendered_line_count: usize,
  pub markdown_has_pannable_lines: bool,
  pub file_scroll: usize,
  pub pane_focus: RepoPane,
  pub status_message: Option<String>,
  pub no_token: bool,
  /// Horizontal character offset for panning (file pane only).
  pub h_offset: usize,
  /// Effective render width (0 = use pane width). +/- keys adjust this.
  pub wrap_width: usize,
  /// Momentum scroll velocity (lines/frame). Positive = down.
  pub scroll_velocity: f32,
}

/// Result from a background repo fetch operation.
pub enum RepoFetchResult {
  /// Initial repo open: default branch + root tree.
  RepoOpened {
    branch: String,
    tree: Result<Vec<crate::github::TreeNode>, String>,
  },
  /// Dir navigation (forward or back): path + tree.
  DirLoaded {
    path: String,
    result: Result<Vec<crate::github::TreeNode>, String>,
  },
  /// File view: path, filename, raw content.
  FileLoaded { path: String, name: String, result: Result<String, String> },
}

/// Result from the URL discovery pipeline.
#[derive(Clone)]
pub enum DiscoverResult {
  ArxivCategory(String),
  HuggingFaceAlreadyEnabled,
  RssFeed { url: String, name: String },
  Failed(String),
}

/// State machine for the "Add source" input in the sources popup.
pub enum SourcesDetectState {
  Idle,
  Detecting,
  Result(DiscoverResult),
}

/// Identifies a pane in the pane registry.
#[derive(PartialEq, Clone, Copy, Debug)]
pub enum PaneId {
  Feed,
  Reader,
  Notes,
  Details,
  Chat,
}

/// Tracks a pane's current screen position and open state.
#[derive(Clone)]
pub struct PaneInfo {
  pub id: PaneId,
  pub rect: Rect,
  pub is_open: bool,
}

impl PaneInfo {
  fn new(id: PaneId) -> Self {
    Self { id, rect: Rect::default(), is_open: false }
  }
}

/// Direction for spatial pane navigation.
#[derive(Clone, Copy, Debug)]
pub enum NavDirection {
  Left,
  Right,
  Up,
  Down,
}

#[derive(PartialEq)]
pub enum AppView {
  Feed,
  Settings,
  Sources,
  RepoViewer,
}

#[derive(PartialEq, Clone, Copy)]
pub enum FeedTab {
  Inbox,
  Discoveries,
}

/// What action should be taken when Enter is pressed in the repo tree pane.
pub enum RepoEnterTarget {
  Dir(String),
  File(String, String), // path, filename
}

pub struct App {
  pub should_quit: bool,

  pub items: Vec<FeedItem>,
  pub selected_index: usize,
  pub list_offset: usize,
  pub discovery_items: Vec<FeedItem>,
  pub discovery_selected_index: usize,
  pub discovery_list_offset: usize,
  pub discovery_rx: Option<Receiver<DiscoveryMessage>>,
  pub discovery_plan: Option<DiscoveryPlan>,
  pub feed_tab: FeedTab,
  pub discovery_loading: bool,
  pub search_query: String,
  pub search_active: bool,
  pub status_message: Option<String>,
  pub persisted_states: HashMap<String, WorkflowState>,

  // Pane focus

  // Filter panel
  pub filter_focus: bool,
  pub filter_cursor: usize,
  pub active_filters: FilterState,

  // Background fetching
  pub fetch_rx: Option<Receiver<FetchMessage>>,
  pub loading_sources: Vec<String>,
  pub loaded_sources: Vec<String>,
  pub is_loading: bool,
  pub spinner_frame: usize,

  // View state
  pub view: AppView,

  // Repo viewer
  pub repo_context: Option<RepoContext>,
  pub github_token: Option<String>,

  // Manual refresh
  pub is_refreshing: bool,

  // Details panel
  pub notification: Option<String>,
  pub notification_item_id: Option<String>,
  pub details_scroll: usize,
  pub details_max_scroll: usize,
  /// URL of the item that was selected when details_scroll was last set.
  /// Used to reset scroll when the user moves to a different item.
  pub details_last_item_url: Option<String>,

  // Config (full, persisted)
  pub config: Config,

  // Settings screen
  pub settings_field: usize,
  pub settings_editing: bool,
  pub settings_edit_buf: String,
  pub settings_github_token: String,
  pub settings_s2_key: String,
  pub settings_save_time: Option<std::time::Instant>,

  // Sources popup
  pub sources_cursor: usize,
  pub sources_input: String,
  pub sources_input_active: bool,
  pub sources_detect_state: SourcesDetectState,
  pub sources_detect_rx: Option<std::sync::mpsc::Receiver<DiscoverResult>>,

  // Embedded notes pane
  pub notes_app: Option<notes::app::App>,
  pub notes_active: bool,

  // Embedded chat pane
  pub chat_ui: Option<chat::ChatUi>,
  pub chat_active: bool,
  pub chat_fullscreen: bool,
  pub chat_at_top: bool,

  // Embedded reader (hygg-reader) in left pane
  pub reader: Option<cli_text_reader::Editor>,
  pub reader_active: bool,

  // Settings buffers for chat fields
  pub settings_claude_key: String,
  pub settings_openai_key: String,
  pub settings_default_chat_provider: String,

  // Background fulltext fetch (article reader)
  pub fulltext_rx: Option<Receiver<Result<Vec<String>, String>>>,
  pub fulltext_loading: bool,

  // Background repo fetch (repo viewer)
  pub repo_fetch_rx: Option<Receiver<RepoFetchResult>>,

  // Scroll debounce — prevents key-repeat and trackpad inertia flooding
  pub last_scroll_time: Option<Instant>,
  pub scroll_debounce_ms: u64,
  pub last_mouse_scroll_time: Option<Instant>,
  pub mouse_scroll_debounce_ms: u64,

  // Leader key + pane registry
  pub leader_active: bool,
  pub leader_activated_at: Option<Instant>,
  pub leader_timeout_ms: u64,
  pub focused_pane: PaneId,
  pub panes: Vec<PaneInfo>,

  // Help overlay
  pub help_active: bool,
  pub help_section: usize,
  pub help_scroll: u16,
}

impl App {
  pub fn new() -> Self {
    Self {
      should_quit: false,
      items: Vec::new(),
      selected_index: 0,
      list_offset: 0,
      discovery_items: crate::store::discovery_cache::load(),
      discovery_selected_index: 0,
      discovery_list_offset: 0,
      discovery_rx: None,
      discovery_plan: None,
      feed_tab: FeedTab::Inbox,
      discovery_loading: false,
      search_query: String::new(),
      search_active: false,
      status_message: None,
      persisted_states: HashMap::new(),
      fetch_rx: None,
      loading_sources: Vec::new(),
      loaded_sources: Vec::new(),
      is_loading: false,
      spinner_frame: 0,
      filter_focus: false,
      filter_cursor: 0,
      active_filters: FilterState::new(),
      view: AppView::Feed,
      repo_context: None,
      github_token: None,
      is_refreshing: false,
      notification: None,
      notification_item_id: None,
      details_scroll: 0,
      details_max_scroll: usize::MAX,
      details_last_item_url: None,
      config: Config::default(),
      settings_field: 0,
      settings_editing: false,
      settings_edit_buf: String::new(),
      settings_github_token: String::new(),
      settings_s2_key: String::new(),
      settings_save_time: None,
      sources_cursor: 0,
      sources_input: String::new(),
      sources_input_active: false,
      sources_detect_state: SourcesDetectState::Idle,
      sources_detect_rx: None,
      notes_app: None,
      notes_active: false,
      chat_ui: None,
      chat_active: false,
      chat_fullscreen: false,
      chat_at_top: false,
      reader: None,
      reader_active: false,
      settings_claude_key: String::new(),
      settings_openai_key: String::new(),
      settings_default_chat_provider: "claude".to_string(),
      fulltext_rx: None,
      fulltext_loading: false,
      repo_fetch_rx: None,
      last_scroll_time: None,
      scroll_debounce_ms: 50,
      last_mouse_scroll_time: None,
      mouse_scroll_debounce_ms: 80,
      leader_active: false,
      leader_activated_at: None,
      leader_timeout_ms: 1000,
      focused_pane: PaneId::Feed,
      help_active: false,
      help_section: 0,
      help_scroll: 0,
      panes: vec![
        PaneInfo::new(PaneId::Feed),
        PaneInfo::new(PaneId::Reader),
        PaneInfo::new(PaneId::Notes),
        PaneInfo::new(PaneId::Details),
        PaneInfo::new(PaneId::Chat),
      ],
    }
  }

  // ── Pane registry ──────────────────────────────────────────────────────────

  /// Called from layout every frame with the computed screen rects.
  /// Pass `None` for a pane that is not currently rendered.
  pub fn update_pane_rects(
    &mut self,
    feed: Option<Rect>,
    reader: Option<Rect>,
    notes: Option<Rect>,
    details: Option<Rect>,
    chat: Option<Rect>,
  ) {
    let updates: [(PaneId, Option<Rect>); 5] = [
      (PaneId::Feed, feed),
      (PaneId::Reader, reader),
      (PaneId::Notes, notes),
      (PaneId::Details, details),
      (PaneId::Chat, chat),
    ];
    for (id, opt) in updates {
      if let Some(info) = self.panes.iter_mut().find(|p| p.id == id) {
        info.is_open = opt.is_some();
        if let Some(r) = opt {
          info.rect = r;
        }
      }
    }
  }

  /// Returns the `PaneId` of the nearest open pane in the given direction,
  /// using center-to-center Euclidean distance among directional candidates.
  pub fn find_pane_in_direction(&self, dir: NavDirection) -> Option<PaneId> {
    let current =
      self.panes.iter().find(|p| p.id == self.focused_pane && p.is_open)?;
    let cx = current.rect.x as i32 + current.rect.width as i32 / 2;
    let cy = current.rect.y as i32 + current.rect.height as i32 / 2;

    self
      .panes
      .iter()
      .filter(|p| {
        p.id != self.focused_pane
          && p.is_open
          && p.rect.width > 0
          && p.rect.height > 0
      })
      .filter(|p| {
        let px = p.rect.x as i32;
        let py = p.rect.y as i32;
        let pw = p.rect.width as i32;
        let ph = p.rect.height as i32;
        match dir {
          NavDirection::Right => px + pw / 2 > cx,
          NavDirection::Left => px + pw / 2 < cx,
          NavDirection::Down => py + ph / 2 > cy,
          NavDirection::Up => py + ph / 2 < cy,
        }
      })
      .min_by_key(|p| {
        let pcx = p.rect.x as i32 + p.rect.width as i32 / 2;
        let pcy = p.rect.y as i32 + p.rect.height as i32 / 2;
        (pcx - cx) * (pcx - cx) + (pcy - cy) * (pcy - cy)
      })
      .map(|p| p.id)
  }

  /// Returns the `PaneId` of the open pane whose rect contains the given
  /// terminal cell, or `None` if no open pane covers that cell.
  pub fn pane_at(&self, col: u16, row: u16) -> Option<PaneId> {
    self
      .panes
      .iter()
      .filter(|p| p.is_open && p.rect.width > 0 && p.rect.height > 0)
      .find(|p| {
        col >= p.rect.x
          && col < p.rect.x + p.rect.width
          && row >= p.rect.y
          && row < p.rect.y + p.rect.height
      })
      .map(|p| p.id)
  }

  /// Returns secondary open panes sorted top-to-bottom then left-to-right.
  pub fn secondary_panes_sorted(&self) -> Vec<PaneId> {
    let primary =
      if self.reader_active { PaneId::Reader } else { PaneId::Feed };
    let mut secondaries: Vec<&PaneInfo> =
      self.panes.iter().filter(|p| p.id != primary && p.is_open).collect();
    secondaries.sort_by_key(|p| (p.rect.y, p.rect.x));
    secondaries.iter().map(|p| p.id).collect()
  }

  /// Items visible after applying search and category filters.
  pub fn visible_items(&self) -> Vec<&FeedItem> {
    let q = self.search_query.to_lowercase();
    self
      .items_for_tab()
      .iter()
      .filter(|item| {
        // Hide items whose source is explicitly disabled in config.
        // HuggingFace items carry source_name="hf"; map to the config key.
        let key = if item.source_platform
          == crate::models::SourcePlatform::HuggingFace
        {
          "huggingface"
        } else {
          &item.source_name
        };
        if let Some(&enabled) = self.config.sources.enabled_sources.get(key) {
          if !enabled {
            return false;
          }
        }

        if !q.is_empty() {
          if !item.title.to_lowercase().contains(&q)
            && !item.authors.iter().any(|a| a.to_lowercase().contains(&q))
          {
            return false;
          }
        }
        self.active_filters.matches(item)
      })
      .collect()
  }

  pub fn items_for_tab(&self) -> &[FeedItem] {
    match self.feed_tab {
      FeedTab::Inbox => &self.items,
      FeedTab::Discoveries => &self.discovery_items,
    }
  }

  fn items_for_tab_mut(&mut self) -> &mut Vec<FeedItem> {
    match self.feed_tab {
      FeedTab::Inbox => &mut self.items,
      FeedTab::Discoveries => &mut self.discovery_items,
    }
  }

  pub fn active_selected_index(&self) -> usize {
    match self.feed_tab {
      FeedTab::Inbox => self.selected_index,
      FeedTab::Discoveries => self.discovery_selected_index,
    }
  }

  pub fn active_list_offset(&self) -> usize {
    match self.feed_tab {
      FeedTab::Inbox => self.list_offset,
      FeedTab::Discoveries => self.discovery_list_offset,
    }
  }

  pub fn set_active_selected_index(&mut self, value: usize) {
    match self.feed_tab {
      FeedTab::Inbox => self.selected_index = value,
      FeedTab::Discoveries => self.discovery_selected_index = value,
    }
  }

  pub fn set_active_list_offset(&mut self, value: usize) {
    match self.feed_tab {
      FeedTab::Inbox => self.list_offset = value,
      FeedTab::Discoveries => self.discovery_list_offset = value,
    }
  }

  pub fn reset_active_feed_position(&mut self) {
    self.set_active_selected_index(0);
    self.set_active_list_offset(0);
    self.details_scroll = 0;
    self.details_last_item_url = None;
  }

  pub fn set_notification(&mut self, msg: String) {
    let url = self.selected_item().map(|i| i.url.clone());
    self.notification = Some(msg);
    self.notification_item_id = url;
  }

  pub fn clear_notification(&mut self) {
    self.notification = None;
    self.notification_item_id = None;
  }

  pub fn move_down(&mut self) {
    let len = self.visible_items().len();
    if len == 0 {
      return;
    }
    let next = (self.active_selected_index() + 1).min(len - 1);
    self.set_active_selected_index(next);
    self.details_scroll = 0;
    self.clear_notification();
  }

  pub fn move_up(&mut self) {
    self.set_active_selected_index(
      self.active_selected_index().saturating_sub(1),
    );
    self.details_scroll = 0;
    self.clear_notification();
  }

  pub fn go_to_top(&mut self) {
    self.set_active_selected_index(0);
    self.details_scroll = 0;
    self.clear_notification();
  }

  pub fn go_to_bottom(&mut self) {
    let len = self.visible_items().len();
    if len > 0 {
      self.set_active_selected_index(len - 1);
    }
    self.details_scroll = 0;
    self.clear_notification();
  }

  pub fn details_scroll_down(&mut self) {
    self.details_scroll =
      self.details_scroll.saturating_add(1).min(self.details_max_scroll);
  }

  /// Called by the renderer each frame with the computed max scroll for the
  /// details pane. Keeps `details_scroll` bounded without the renderer needing
  /// to mutate scroll state itself.
  pub fn set_details_max_scroll(&mut self, max: usize) {
    self.details_max_scroll = max;
    if self.details_scroll > max {
      self.details_scroll = max;
    }
  }

  pub fn details_scroll_up(&mut self) {
    self.details_scroll = self.details_scroll.saturating_sub(1);
  }

  pub fn push_search_char(&mut self, c: char) {
    self.search_query.push(c);
    self.reset_active_feed_position();
  }

  pub fn pop_search_char(&mut self) {
    self.search_query.pop();
    self.reset_active_feed_position();
  }

  pub fn selected_item(&self) -> Option<&FeedItem> {
    self.visible_items().into_iter().nth(self.active_selected_index())
  }

  pub fn handle_slash_command(&mut self, cmd: String) {
    let parsed = crate::commands::parser::parse_slash_command(&cmd);
    crate::commands::dispatch::dispatch_slash_command(self, parsed);
  }

  pub fn push_chat_assistant_message(&mut self, content: String) {
    if let Some(chat_ui) = self.chat_ui.as_mut() {
      if let Some(session) = chat_ui.active_session.as_mut() {
        session.messages.push(chat::ChatMessage {
          role: chat::Role::Assistant,
          content,
          timestamp: Utc::now(),
        });
        session.updated_at = Utc::now();
        let _ = chat::save_session(session);
        let meta = chat::storage::session_to_meta(session);
        let id = meta.id.clone();
        if let Some(pos) = chat_ui.sessions.iter().position(|s| s.id == id) {
          chat_ui.sessions[pos] = meta;
        }
        let index = chat::ChatIndex {
          sessions: chat_ui.sessions.clone(),
          default_provider: chat_ui.default_provider.clone(),
        };
        let _ = chat::save_index(&index);
        chat_ui.scroll_offset = usize::MAX;
      }
    }
  }

  pub fn clear_chat_messages(&mut self) {
    if let Some(chat_ui) = self.chat_ui.as_mut() {
      if let Some(session) = chat_ui.active_session.as_mut() {
        session.messages.clear();
        session.updated_at = Utc::now();
        let _ = chat::save_session(session);
        let meta = chat::storage::session_to_meta(session);
        let id = meta.id.clone();
        if let Some(pos) = chat_ui.sessions.iter().position(|s| s.id == id) {
          chat_ui.sessions[pos] = meta;
        }
        let index = chat::ChatIndex {
          sessions: chat_ui.sessions.clone(),
          default_provider: chat_ui.default_provider.clone(),
        };
        let _ = chat::save_index(&index);
        chat_ui.scroll_offset = 0;
      }
    }
  }

  // ── Repo viewer ────────────────────────────────────────────────────────

  pub fn close_repo_viewer(&mut self) {
    self.view = AppView::Feed;
    self.repo_context = None;
  }

  pub fn set_repo_status(&mut self, msg: impl Into<String>) {
    if let Some(ctx) = &mut self.repo_context {
      ctx.status_message = Some(msg.into());
    }
  }

  /// Returns the action to take when Enter is pressed in the tree pane.
  pub fn repo_enter_target(&self) -> Option<RepoEnterTarget> {
    let ctx = self.repo_context.as_ref()?;
    if ctx.no_token {
      return None;
    }
    let node = ctx.tree_nodes.get(ctx.tree_cursor)?;
    match node.node_type {
      crate::github::NodeType::Dir => {
        Some(RepoEnterTarget::Dir(node.path.clone()))
      }
      crate::github::NodeType::File => {
        Some(RepoEnterTarget::File(node.path.clone(), node.name.clone()))
      }
    }
  }

  /// Returns the parent path for `b` (go up), or None if already at root.
  pub fn repo_back_target(&self) -> Option<String> {
    let ctx = self.repo_context.as_ref()?;
    if ctx.no_token || ctx.tree_path.is_empty() {
      return None;
    }
    let parent = match ctx.tree_path.rfind('/') {
      Some(pos) => ctx.tree_path[..pos].to_string(),
      None => String::new(),
    };
    Some(parent)
  }

  pub fn repo_apply_dir(
    &mut self,
    path: String,
    result: Result<Vec<crate::github::TreeNode>, String>,
  ) {
    let ctx = match self.repo_context.as_mut() {
      Some(c) => c,
      None => return,
    };
    match result {
      Ok(nodes) => {
        ctx.tree_path = path;
        ctx.tree_nodes = nodes;
        ctx.tree_cursor = 0;
        ctx.pane_focus = RepoPane::Tree;
        ctx.status_message = None;
      }
      Err(e) => {
        ctx.status_message = Some(format!("Error: {e}"));
      }
    }
  }

  pub fn repo_apply_file(
    &mut self,
    path: String,
    name: String,
    result: Result<String, String>,
  ) {
    let ctx = match self.repo_context.as_mut() {
      Some(c) => c,
      None => return,
    };
    match result {
      Ok(raw_content) => {
        let file_kind = classify_repo_file_kind(&name, &raw_content);
        let highlighted = match file_kind {
          RepoFileKind::Code => {
            crate::syntax::highlight_file(&raw_content, &name)
              .unwrap_or_default()
          }
          _ => Vec::new(),
        };
        let lines: Vec<String> =
          raw_content.lines().map(|l| l.to_string()).collect();
        ctx.file_path = Some(path);
        ctx.file_name = Some(name);
        ctx.raw_file_content = raw_content;
        ctx.file_kind = file_kind;
        ctx.file_lines = lines;
        ctx.file_highlighted = highlighted;
        ctx.markdown_cache = None;
        ctx.rendered_line_count = 0;
        ctx.markdown_has_pannable_lines = false;
        ctx.file_scroll = 0;
        ctx.h_offset = 0;
        ctx.scroll_velocity = 0.0;
        ctx.pane_focus = RepoPane::File;
        ctx.status_message = None;
      }
      Err(e) => {
        ctx.status_message = Some(format!("Error: {e}"));
      }
    }
  }

  pub fn repo_switch_pane(&mut self) {
    if let Some(ctx) = &mut self.repo_context {
      ctx.pane_focus = match ctx.pane_focus {
        RepoPane::Tree => RepoPane::File,
        RepoPane::File => RepoPane::Tree,
      };
    }
  }

  pub fn repo_nav_down(&mut self, file_visible_h: usize) {
    let _ = file_visible_h;
    if let Some(ctx) = &mut self.repo_context {
      match ctx.pane_focus {
        RepoPane::Tree => {
          let max = ctx.tree_nodes.len().saturating_sub(1);
          ctx.tree_cursor = (ctx.tree_cursor + 1).min(max);
        }
        RepoPane::File => {
          ctx.scroll_velocity += 3.0;
        }
      }
    }
  }

  pub fn repo_nav_up(&mut self) {
    if let Some(ctx) = &mut self.repo_context {
      match ctx.pane_focus {
        RepoPane::Tree => {
          ctx.tree_cursor = ctx.tree_cursor.saturating_sub(1);
        }
        RepoPane::File => {
          ctx.scroll_velocity -= 3.0;
        }
      }
    }
  }

  /// Advance momentum scroll by one frame.
  pub fn repo_tick(&mut self) {
    if let Some(ctx) = &mut self.repo_context {
      if ctx.scroll_velocity.abs() >= 0.5 {
        let delta = ctx.scroll_velocity.round() as i64;
        let line_count = match ctx.file_kind {
          RepoFileKind::Markdown => ctx.rendered_line_count,
          _ => ctx.file_lines.len(),
        };
        let max = line_count.saturating_sub(1) as i64;
        let next = (ctx.file_scroll as i64 + delta).clamp(0, max) as usize;
        ctx.file_scroll = next;
        ctx.scroll_velocity *= 0.75;
      } else {
        ctx.scroll_velocity = 0.0;
      }
    }
  }

  pub fn repo_pan_left(&mut self) {
    if let Some(ctx) = &mut self.repo_context {
      if ctx.file_kind == RepoFileKind::Markdown
        && !ctx.markdown_has_pannable_lines
      {
        return;
      }
      ctx.h_offset = ctx.h_offset.saturating_sub(4);
    }
  }

  pub fn repo_pan_right(&mut self) {
    if let Some(ctx) = &mut self.repo_context {
      if ctx.file_kind == RepoFileKind::Markdown
        && !ctx.markdown_has_pannable_lines
      {
        return;
      }
      ctx.h_offset += 4;
    }
  }

  pub fn repo_zoom_in(&mut self) {
    if let Some(ctx) = &mut self.repo_context {
      if ctx.file_kind != RepoFileKind::Markdown {
        return;
      }
      if ctx.wrap_width == 0 {
        // start from a sensible default — we don't know pane width here
        ctx.wrap_width = 120;
      }
      ctx.wrap_width = ctx.wrap_width.saturating_sub(10).max(20);
    }
  }

  pub fn repo_zoom_out(&mut self) {
    if let Some(ctx) = &mut self.repo_context {
      if ctx.file_kind != RepoFileKind::Markdown {
        return;
      }
      if ctx.wrap_width == 0 {
        ctx.wrap_width = 80;
      }
      ctx.wrap_width += 10;
    }
  }

  /// Copy the currently selected path to clipboard.
  pub fn repo_copy_path(&mut self) {
    let path = if let Some(ctx) = &self.repo_context {
      match ctx.pane_focus {
        RepoPane::File => {
          ctx.file_path.clone().unwrap_or_else(|| ctx.tree_path.clone())
        }
        RepoPane::Tree => ctx
          .tree_nodes
          .get(ctx.tree_cursor)
          .map(|n| n.path.clone())
          .unwrap_or_default(),
      }
    } else {
      return;
    };

    match arboard::Clipboard::new() {
      Ok(mut cb) => match cb.set_text(&path) {
        Ok(()) => self.set_repo_status(format!("Copied: {path}")),
        Err(e) => self.set_repo_status(format!("Clipboard error: {e}")),
      },
      Err(e) => self.set_repo_status(format!("Clipboard unavailable: {e}")),
    }
  }

  /// Save the current open file to ~/Downloads/{filename}.
  pub fn repo_download_file(&mut self) {
    let (name, content) = if let Some(ctx) = &self.repo_context {
      match (&ctx.file_name, &ctx.file_lines) {
        (Some(name), lines) if !lines.is_empty() => {
          (name.clone(), lines.join("\n"))
        }
        _ => return,
      }
    } else {
      return;
    };

    let dest =
      dirs::download_dir().or_else(dirs::home_dir).map(|p| p.join(&name));

    if let Some(path) = dest {
      match std::fs::write(&path, &content) {
        Ok(()) => self.set_repo_status(format!("Saved to {}", path.display())),
        Err(e) => self.set_repo_status(format!("Download failed: {e}")),
      }
    }
  }

  pub fn process_incoming(&mut self) {
    use std::sync::mpsc::TryRecvError;

    self.spinner_frame = self.spinner_frame.wrapping_add(1);
    self.poll_detect_result();
    self.process_incoming_discovery();

    // Clear "Saved." confirmation after 2 seconds.
    if let Some(t) = self.settings_save_time {
      if t.elapsed().as_secs() >= 2 {
        self.settings_save_time = None;
      }
    }

    if self.fetch_rx.is_none() {
      return;
    }

    // Collect pending messages without blocking.
    let mut messages = Vec::new();
    let mut disconnected = false;

    if let Some(rx) = &self.fetch_rx {
      loop {
        match rx.try_recv() {
          Ok(msg) => messages.push(msg),
          Err(TryRecvError::Empty) => break,
          Err(TryRecvError::Disconnected) => {
            disconnected = true;
            break;
          }
        }
      }
    }

    if disconnected {
      self.is_loading = false;
      self.is_refreshing = false;
      self.fetch_rx = None;
    }

    let was_empty = self.items.is_empty();
    let mut had_items = false;

    for msg in messages {
      match msg {
        FetchMessage::Items(new_items) => {
          had_items = true;
          for mut item in new_items {
            // Apply any persisted workflow state.
            if let Some(state) = self.persisted_states.get(&item.url) {
              item.workflow_state = *state;
            }

            // URL dedup: overwrite cached item with freshly fetched data.
            // workflow_state was already set from persisted_states above.
            if let Some(existing) =
              self.items.iter_mut().find(|i| i.url == item.url)
            {
              *existing = item;
              continue;
            }

            // ArXiv ID dedup: collapse HF and arXiv entries for the same paper.
            // Keep the arXiv entry as primary.
            let aid = arxiv_id_from_url(&item.url);
            if let Some(ref aid) = aid {
              let pos = self.items.iter().position(|i| {
                arxiv_id_from_url(&i.url).as_deref() == Some(aid.as_str())
              });
              if let Some(pos) = pos {
                if item.source_platform == SourcePlatform::ArXiv {
                  // Incoming is the canonical arXiv entry — replace HF stub.
                  let ws = self.items[pos].workflow_state;
                  self.items[pos] = item;
                  self.items[pos].workflow_state = ws;
                }
                // else: existing is already arXiv, drop the HF duplicate.
                continue;
              }
            }

            self.items.push(item);
          }
        }
        FetchMessage::SourceComplete(name) => {
          self.loading_sources.retain(|s| s != &name);
          self.loaded_sources.push(name);
        }
        FetchMessage::SourceError(name, err) => {
          self.status_message = Some(err);
          self.loading_sources.retain(|s| s != &name);
        }
        FetchMessage::AllComplete => {
          self.is_loading = false;
          self.is_refreshing = false;
        }
      }
    }

    if had_items {
      self.items.sort_by(|a, b| b.published_at.cmp(&a.published_at));
      crate::store::cache::save(&self.items);
      if was_empty {
        self.list_offset = 0;
      }
    }
  }

  pub fn process_incoming_discovery(&mut self) {
    use std::sync::mpsc::TryRecvError;

    let mut messages = Vec::new();
    let mut disconnected = false;

    if let Some(rx) = &self.discovery_rx {
      loop {
        match rx.try_recv() {
          Ok(msg) => messages.push(msg),
          Err(TryRecvError::Empty) => break,
          Err(TryRecvError::Disconnected) => {
            disconnected = true;
            break;
          }
        }
      }
    }

    if disconnected {
      self.discovery_rx = None;
      self.discovery_loading = false;
    }

    for msg in messages {
      match msg {
        DiscoveryMessage::PlanReady(plan) => {
          let checklist = format_discovery_plan_message(&plan);
          self.discovery_plan = Some(plan);
          self.push_chat_assistant_message(checklist);
        }
        DiscoveryMessage::Items(items) => {
          self.merge_discovery_items(items);
          save_discovery_items(&self.discovery_items);
        }
        DiscoveryMessage::Complete => {
          self.discovery_rx = None;
          self.discovery_loading = false;
          self.status_message = Some("Discovery complete".to_string());
        }
        DiscoveryMessage::Error(e) => {
          self.discovery_rx = None;
          self.discovery_loading = false;
          self.push_chat_assistant_message(format!("Discovery failed: {e}"));
          self.status_message = Some("Discovery failed".to_string());
        }
      }
    }
  }

  fn merge_discovery_items(&mut self, items: Vec<FeedItem>) {
    for mut item in items {
      if let Some(state) = self.persisted_states.get(&item.url) {
        item.workflow_state = *state;
      }

      if let Some(existing) =
        self.discovery_items.iter_mut().find(|i| i.url == item.url)
      {
        *existing = item;
        continue;
      }

      let aid = arxiv_id_from_url(&item.url);
      if let Some(ref aid) = aid {
        let pos = self.discovery_items.iter().position(|i| {
          arxiv_id_from_url(&i.url).as_deref() == Some(aid.as_str())
        });
        if let Some(pos) = pos {
          if item.source_platform == SourcePlatform::ArXiv {
            let ws = self.discovery_items[pos].workflow_state;
            self.discovery_items[pos] = item;
            self.discovery_items[pos].workflow_state = ws;
          }
          continue;
        }
      }

      self.discovery_items.push(item);
    }
    self.discovery_items.sort_by(|a, b| b.published_at.cmp(&a.published_at));
  }

  pub fn filter_cursor_down(&mut self) {
    let max = self.filter_total_items().saturating_sub(1);
    self.filter_cursor = (self.filter_cursor + 1).min(max);
  }

  pub fn filter_cursor_up(&mut self) {
    self.filter_cursor = self.filter_cursor.saturating_sub(1);
  }

  /// Total number of selectable rows in the filter panel (dynamic source count + fixed).
  pub fn filter_total_items(&self) -> usize {
    self.filter_source_names().len() + 3 + 3 + 5 + 1
  }

  /// Sorted unique source label strings derived from loaded items.
  pub fn filter_source_names(&self) -> Vec<String> {
    let mut names: std::collections::BTreeSet<String> = self
      .items
      .iter()
      .map(|item| {
        if item.source_name.is_empty() {
          item.source_platform.short_label().to_string()
        } else {
          item.source_name.clone()
        }
      })
      .collect();
    // Always include at least "arxiv" and "hf" so the filter is useful
    // even before a fetch completes.
    for seed in &["arxiv", "hf"] {
      names.insert(seed.to_string());
    }
    names.into_iter().collect()
  }

  pub fn toggle_filter_at_cursor(&mut self) {
    let source_names = self.filter_source_names();
    let src_count = source_names.len();
    let c = self.filter_cursor;

    if c < src_count {
      let name = source_names[c].clone();
      if !self.active_filters.sources.remove(&name) {
        self.active_filters.sources.insert(name);
      }
    } else {
      match c - src_count {
        0 => toggle_set(&mut self.active_filters.signals, SignalLevel::Primary),
        1 => {
          toggle_set(&mut self.active_filters.signals, SignalLevel::Secondary)
        }
        2 => {
          toggle_set(&mut self.active_filters.signals, SignalLevel::Tertiary)
        }
        3 => {
          toggle_set(&mut self.active_filters.content_types, ContentType::Paper)
        }
        4 => toggle_set(
          &mut self.active_filters.content_types,
          ContentType::Article,
        ),
        5 => toggle_set(
          &mut self.active_filters.content_types,
          ContentType::Digest,
        ),
        6 => toggle_set(
          &mut self.active_filters.workflow_states,
          WorkflowState::Inbox,
        ),
        7 => toggle_set(
          &mut self.active_filters.workflow_states,
          WorkflowState::Skimmed,
        ),
        8 => toggle_set(
          &mut self.active_filters.workflow_states,
          WorkflowState::Queued,
        ),
        9 => toggle_set(
          &mut self.active_filters.workflow_states,
          WorkflowState::DeepRead,
        ),
        10 => toggle_set(
          &mut self.active_filters.workflow_states,
          WorkflowState::Archived,
        ),
        11 => self.active_filters = FilterState::new(),
        _ => {}
      }
    }
    self.reset_active_feed_position();
  }

  pub fn clear_filters(&mut self) {
    self.active_filters = FilterState::new();
    self.reset_active_feed_position();
  }

  // ── Sources popup ───────────────────────────────────────────────────────

  /// arXiv categories to display in the popup: known list + any user extras.
  pub fn sources_popup_arxiv_cats(&self) -> Vec<(String, String)> {
    let mut cats: Vec<(String, String)> = crate::config::KNOWN_ARXIV_CATS
      .iter()
      .map(|(code, label)| (code.to_string(), label.to_string()))
      .collect();
    for cat in &self.config.sources.arxiv_categories {
      if !crate::config::KNOWN_ARXIV_CATS
        .iter()
        .any(|(k, _)| *k == cat.as_str())
      {
        cats.push((cat.clone(), String::new()));
      }
    }
    cats
  }

  /// Total number of cursor-navigable rows in the sources popup.
  pub fn sources_popup_total_items(&self) -> usize {
    1 // input field
      + self.sources_popup_arxiv_cats().len()
      + crate::config::PREDEFINED_SOURCES.len()
      + self.config.sources.custom_feeds.len()
  }

  /// Poll the discovery background thread and update detect state.
  pub fn poll_detect_result(&mut self) {
    use std::sync::mpsc::TryRecvError;
    let result = if let Some(rx) = &self.sources_detect_rx {
      Some(rx.try_recv())
    } else {
      None
    };
    match result {
      Some(Ok(r)) => {
        self.sources_detect_state = SourcesDetectState::Result(r);
        self.sources_detect_rx = None;
      }
      Some(Err(TryRecvError::Disconnected)) => {
        self.sources_detect_state = SourcesDetectState::Result(
          DiscoverResult::Failed("Detection thread disconnected".to_string()),
        );
        self.sources_detect_rx = None;
      }
      _ => {}
    }
  }

  pub fn set_workflow_state(&mut self, state: WorkflowState) {
    // Collect the URL of the currently selected visible item
    let url = {
      let visible = self.visible_items();
      visible.get(self.active_selected_index()).map(|item| item.url.clone())
    };

    if let Some(url) = url {
      if let Some(item) =
        self.items_for_tab_mut().iter_mut().find(|i| i.url == url)
      {
        item.workflow_state = state;
      }
      self.persisted_states.insert(url, state);
      crate::store::save(&self.persisted_states);
    }
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn toggle_set<T: Eq + std::hash::Hash>(set: &mut HashSet<T>, value: T) {
  if !set.remove(&value) {
    set.insert(value);
  }
}


fn save_discovery_items(_items: &[FeedItem]) {
  crate::store::discovery_cache::save(_items);
}

fn format_discovery_plan_message(plan: &DiscoveryPlan) -> String {
  let cats = if plan.arxiv_categories.is_empty() {
    "none".to_string()
  } else {
    plan.arxiv_categories.join(" · ")
  };
  let terms = if plan.search_terms.is_empty() {
    "none".to_string()
  } else {
    plan.search_terms.join(" · ")
  };

  let mut lines = vec![
    format!("Discovery: \"{}\"", plan.topic),
    String::new(),
    format!("arXiv categories:  {cats}"),
    format!("Search terms:      {terms}"),
    format!("Papers targeted:   {} specific IDs", plan.paper_ids.len()),
  ];

  lines.push(String::new());
  lines.push("Suggested sources:".to_string());
  if plan.arxiv_categories.is_empty()
    && plan.rss_urls.is_empty()
    && plan.github_sources.is_empty()
    && plan.huggingface_sources.is_empty()
  {
    lines.push("  none".to_string());
  }
  for cat in &plan.arxiv_categories {
    lines.push(format!("  /add {cat}"));
  }
  for feed in &plan.rss_urls {
    if crate::discovery::ai_query::is_http_url(&feed.url) {
      lines.push(format!("  /add-feed {}", feed.url));
    }
  }
  for source in &plan.github_sources {
    if crate::discovery::ai_query::is_http_url(&source.url) {
      lines.push(format!("  [ ] GitHub {} {}", source.kind, source.url));
    }
  }
  for source in &plan.huggingface_sources {
    if crate::discovery::ai_query::is_http_url(&source.url) {
      lines.push(format!("  [ ] HuggingFace {} {}", source.kind, source.url));
    }
  }

  lines.extend([
    String::new(),
    "To add sources permanently:".to_string(),
    "  /add cs.LG".to_string(),
    "  /add-feed URL".to_string(),
    "  /clear discoveries".to_string(),
  ]);

  lines.join("\n")
}

#[cfg(test)]
fn mock_items() -> Vec<FeedItem> {
  vec![
    FeedItem {
      id: "1".into(),
      title: "Attention Is All You Need: Revisited".into(),
      source_platform: SourcePlatform::ArXiv,
      content_type: ContentType::Paper,
      domain_tags: vec!["transformers".into(), "nlp".into()],
      signal: SignalLevel::Primary,
      published_at: "2026-03-15".into(),
      authors: vec!["Vaswani, A.".into(), "Shazeer, N.".into()],
      summary_short: "A retrospective look at the transformer architecture \
        five years on, with ablations on modern hardware."
        .into(),
      workflow_state: WorkflowState::Inbox,
      url: "https://arxiv.org/abs/2603.00001".into(),
      upvote_count: 0,
      github_repo: None,
      github_owner: None,
      github_repo_name: None,
      benchmark_results: vec![],
      full_content: None,
      source_name: String::new(),
    },
    FeedItem {
      id: "2".into(),
      title: "Mamba-2: State Space Models at Scale".into(),
      source_platform: SourcePlatform::ArXiv,
      content_type: ContentType::Paper,
      domain_tags: vec!["ssm".into(), "efficiency".into()],
      signal: SignalLevel::Primary,
      published_at: "2026-03-14".into(),
      authors: vec!["Gu, A.".into(), "Dao, T.".into()],
      summary_short: "Extends Mamba with structured state space duality \
        enabling better scaling laws."
        .into(),
      workflow_state: WorkflowState::Queued,
      url: "https://arxiv.org/abs/2603.00002".into(),
      upvote_count: 0,
      github_repo: None,
      github_owner: None,
      github_repo_name: None,
      benchmark_results: vec![],
      full_content: None,
      source_name: String::new(),
    },
    FeedItem {
      id: "3".into(),
      title: "Flash Attention 3 benchmarks on H100".into(),
      source_platform: SourcePlatform::Twitter,
      content_type: ContentType::Thread,
      domain_tags: vec!["cuda".into(), "attention".into()],
      signal: SignalLevel::Secondary,
      published_at: "2026-03-13".into(),
      authors: vec!["tri_dao".into()],
      summary_short: "Thread covering FA3 throughput numbers versus \
        cuDNN on H100 SXM across sequence lengths."
        .into(),
      workflow_state: WorkflowState::Skimmed,
      url: "https://twitter.com/tri_dao/status/000001".into(),
      upvote_count: 0,
      github_repo: None,
      github_owner: None,
      github_repo_name: None,
      benchmark_results: vec![],
      full_content: None,
      source_name: String::new(),
    },
    FeedItem {
      id: "4".into(),
      title: "Building production RAG pipelines without the hype".into(),
      source_platform: SourcePlatform::Blog,
      content_type: ContentType::Article,
      domain_tags: vec!["rag".into(), "production".into()],
      signal: SignalLevel::Secondary,
      published_at: "2026-03-12".into(),
      authors: vec!["Hamel Husain".into()],
      summary_short: "Practical notes on chunking strategies, reranking, \
        and eval harnesses for retrieval-augmented generation."
        .into(),
      workflow_state: WorkflowState::Inbox,
      url: "https://hamel.dev/blog/rag-prod".into(),
      upvote_count: 0,
      github_repo: None,
      github_owner: None,
      github_repo_name: None,
      benchmark_results: vec![],
      full_content: None,
      source_name: String::new(),
    },
    FeedItem {
      id: "5".into(),
      title: "open-instruct: finetuning LLMs at AllenAI".into(),
      source_platform: SourcePlatform::PapersWithCode,
      content_type: ContentType::Repo,
      domain_tags: vec!["finetuning".into(), "rlhf".into()],
      signal: SignalLevel::Primary,
      published_at: "2026-03-11".into(),
      authors: vec!["AllenAI".into()],
      summary_short: "Open-source recipe for instruction tuning and \
        RLHF used in Tulu 3, with full training configs."
        .into(),
      workflow_state: WorkflowState::DeepRead,
      url: "https://paperswithcode.com/paper/open-instruct".into(),
      upvote_count: 0,
      github_repo: None,
      github_owner: None,
      github_repo_name: None,
      benchmark_results: vec![],
      full_content: None,
      source_name: String::new(),
    },
    FeedItem {
      id: "6".into(),
      title: "The Batch — Issue 247: Agents in the wild".into(),
      source_platform: SourcePlatform::Newsletter,
      content_type: ContentType::Digest,
      domain_tags: vec!["agents".into(), "weekly".into()],
      signal: SignalLevel::Tertiary,
      published_at: "2026-03-10".into(),
      authors: vec!["Andrew Ng".into()],
      summary_short: "Weekly digest covering agentic system deployments, \
        tooling updates, and model releases."
        .into(),
      workflow_state: WorkflowState::Archived,
      url: "https://deeplearning.ai/the-batch/issue-247".into(),
      upvote_count: 0,
      github_repo: None,
      github_owner: None,
      github_repo_name: None,
      benchmark_results: vec![],
      full_content: None,
      source_name: String::new(),
    },
    FeedItem {
      id: "7".into(),
      title: "Constitutional AI: Harmlessness from AI Feedback".into(),
      source_platform: SourcePlatform::ArXiv,
      content_type: ContentType::Paper,
      domain_tags: vec!["alignment".into(), "rlhf".into()],
      signal: SignalLevel::Primary,
      published_at: "2026-03-09".into(),
      authors: vec!["Bai, Y.".into(), "Jones, A.".into()],
      summary_short: "Introduces CAI, a method for training harmless AI \
        assistants using AI-generated feedback without human labels."
        .into(),
      workflow_state: WorkflowState::Queued,
      url: "https://arxiv.org/abs/2212.08073".into(),
      upvote_count: 0,
      github_repo: None,
      github_owner: None,
      github_repo_name: None,
      benchmark_results: vec![],
      full_content: None,
      source_name: String::new(),
    },
    FeedItem {
      id: "8".into(),
      title: "Why every ML team needs an evals culture".into(),
      source_platform: SourcePlatform::Blog,
      content_type: ContentType::Article,
      domain_tags: vec!["evals".into(), "mlops".into()],
      signal: SignalLevel::Secondary,
      published_at: "2026-03-08".into(),
      authors: vec!["Jason Wei".into()],
      summary_short: "Argues for treating evals as first-class engineering, \
        with examples from production LLM deployments."
        .into(),
      workflow_state: WorkflowState::Inbox,
      url: "https://jasonwei.net/blog/evals-culture".into(),
      upvote_count: 0,
      github_repo: None,
      github_owner: None,
      github_repo_name: None,
      benchmark_results: vec![],
      full_content: None,
      source_name: String::new(),
    },
    FeedItem {
      id: "9".into(),
      title:
        "vLLM v0.5 release notes — prefix caching and speculative decoding"
          .into(),
      source_platform: SourcePlatform::PapersWithCode,
      content_type: ContentType::Repo,
      domain_tags: vec!["inference".into(), "serving".into()],
      signal: SignalLevel::Primary,
      published_at: "2026-03-07".into(),
      authors: vec!["vLLM Team".into()],
      summary_short: "v0.5 ships automatic prefix caching and draft-model \
        speculative decoding, cutting median TTFT by 40%."
        .into(),
      workflow_state: WorkflowState::Skimmed,
      url: "https://github.com/vllm-project/vllm/releases/v0.5".into(),
      upvote_count: 0,
      github_repo: None,
      github_owner: None,
      github_repo_name: None,
      benchmark_results: vec![],
      full_content: None,
      source_name: String::new(),
    },
    FeedItem {
      id: "10".into(),
      title: "Mixture of Experts: a practical guide".into(),
      source_platform: SourcePlatform::Newsletter,
      content_type: ContentType::Digest,
      domain_tags: vec!["moe".into(), "architecture".into()],
      signal: SignalLevel::Secondary,
      published_at: "2026-03-06".into(),
      authors: vec!["Sebastian Raschka".into()],
      summary_short: "Deep-dive into MoE routing strategies, load balancing \
        losses, and differences between Switch, GLaM, and Mixtral."
        .into(),
      workflow_state: WorkflowState::Queued,
      url: "https://magazine.sebastianraschka.com/p/moe-guide".into(),
      upvote_count: 0,
      github_repo: None,
      github_owner: None,
      github_repo_name: None,
      benchmark_results: vec![],
      full_content: None,
      source_name: String::new(),
    },
    FeedItem {
      id: "11".into(),
      title: "Context length scaling beyond 1M tokens".into(),
      source_platform: SourcePlatform::Twitter,
      content_type: ContentType::Thread,
      domain_tags: vec!["context".into(), "long-range".into()],
      signal: SignalLevel::Secondary,
      published_at: "2026-03-05".into(),
      authors: vec!["Greg Kamradt".into()],
      summary_short: "Empirical thread on attention sink patterns and \
        retrieval degradation at very long context windows."
        .into(),
      workflow_state: WorkflowState::Inbox,
      url: "https://twitter.com/GregKamradt/status/000002".into(),
      upvote_count: 0,
      github_repo: None,
      github_owner: None,
      github_repo_name: None,
      benchmark_results: vec![],
      full_content: None,
      source_name: String::new(),
    },
    FeedItem {
      id: "12".into(),
      title: "LLM.int8(): 8-bit Matrix Multiplication for Transformers".into(),
      source_platform: SourcePlatform::ArXiv,
      content_type: ContentType::Paper,
      domain_tags: vec!["quantisation".into(), "efficiency".into()],
      signal: SignalLevel::Primary,
      published_at: "2026-03-04".into(),
      authors: vec!["Dettmers, T.".into(), "Lewis, M.".into()],
      summary_short: "Introduces mixed-precision decomposition that preserves \
        full model quality at 8-bit with no fine-tuning."
        .into(),
      workflow_state: WorkflowState::DeepRead,
      url: "https://arxiv.org/abs/2208.07339".into(),
      upvote_count: 0,
      github_repo: None,
      github_owner: None,
      github_repo_name: None,
      benchmark_results: vec![],
      full_content: None,
      source_name: String::new(),
    },
    FeedItem {
      id: "13".into(),
      title: "Toolformer: Language Models Can Teach Themselves to Use Tools"
        .into(),
      source_platform: SourcePlatform::ArXiv,
      content_type: ContentType::Paper,
      domain_tags: vec!["tool-use".into(), "agents".into()],
      signal: SignalLevel::Primary,
      published_at: "2026-03-03".into(),
      authors: vec!["Schick, T.".into()],
      summary_short: "Self-supervised method for teaching LLMs when and how \
        to call APIs, achieving strong performance with few examples."
        .into(),
      workflow_state: WorkflowState::Archived,
      url: "https://arxiv.org/abs/2302.04761".into(),
      upvote_count: 0,
      github_repo: None,
      github_owner: None,
      github_repo_name: None,
      benchmark_results: vec![],
      full_content: None,
      source_name: String::new(),
    },
    FeedItem {
      id: "14".into(),
      title: "Practical notes on GRPO vs PPO for LLM alignment".into(),
      source_platform: SourcePlatform::Blog,
      content_type: ContentType::Article,
      domain_tags: vec!["rl".into(), "alignment".into()],
      signal: SignalLevel::Secondary,
      published_at: "2026-03-02".into(),
      authors: vec!["Leandro von Werra".into()],
      summary_short: "Side-by-side comparison of GRPO and PPO training \
        dynamics, memory usage, and sample efficiency on code tasks."
        .into(),
      workflow_state: WorkflowState::Inbox,
      url: "https://huggingface.co/blog/grpo-vs-ppo".into(),
      upvote_count: 0,
      github_repo: None,
      github_owner: None,
      github_repo_name: None,
      benchmark_results: vec![],
      full_content: None,
      source_name: String::new(),
    },
    FeedItem {
      id: "15".into(),
      title: "axolotl: one config to fine-tune them all".into(),
      source_platform: SourcePlatform::PapersWithCode,
      content_type: ContentType::Repo,
      domain_tags: vec!["finetuning".into(), "tooling".into()],
      signal: SignalLevel::Tertiary,
      published_at: "2026-03-01".into(),
      authors: vec!["Wing Lian".into()],
      summary_short: "Unified fine-tuning framework supporting LoRA, QLoRA, \
        full-param and FSDP across multiple model families."
        .into(),
      workflow_state: WorkflowState::Skimmed,
      url: "https://github.com/OpenAccess-AI-Collective/axolotl".into(),
      upvote_count: 0,
      github_repo: None,
      github_owner: None,
      github_repo_name: None,
      benchmark_results: vec![],
      full_content: None,
      source_name: String::new(),
    },
  ]
}

fn classify_repo_file_kind(name: &str, content: &str) -> RepoFileKind {
  let lower = name.to_ascii_lowercase();
  if lower.ends_with(".md")
    || lower.ends_with(".markdown")
    || lower == "readme"
    || lower.starts_with("readme.")
  {
    return RepoFileKind::Markdown;
  }

  if crate::syntax::highlight_file(content, name).is_some() {
    RepoFileKind::Code
  } else {
    RepoFileKind::PlainText
  }
}
