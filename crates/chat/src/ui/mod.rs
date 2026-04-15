use std::mem;
use std::sync::mpsc;

use chrono::Utc;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
  Frame,
  layout::{Alignment, Constraint, Direction, Layout, Rect},
  style::{Color, Modifier, Style},
  text::{Line, Span, Text},
  widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use crate::{
  ChatIndex, ChatMessage, ChatSession, ChatSessionMeta, Role,
  provider::ProviderResponse,
  provider_registry::{ProviderRegistry, parse_provider_prefix},
  storage::{
    create_session, delete_session, load_index, load_session, save_index,
    save_session,
  },
};

// ── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatUiState {
  SessionList,
  Chat,
  NewSession,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatAction {
  None,
  Quit,
  Sending,
  SlashCommand(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatInputMode {
  Insert,
  Normal,
}

#[derive(Clone, Copy)]
struct SlashCommandSpec {
  command: &'static str,
  completion: &'static str,
  description: &'static str,
}

const SLASH_COMMANDS: &[SlashCommandSpec] = &[
  SlashCommandSpec {
    command: "/discover",
    completion: "/discover ",
    description: "Find papers and sources for a topic",
  },
  SlashCommandSpec {
    command: "/clear discoveries",
    completion: "/clear discoveries",
    description: "Clear the discovery feed",
  },
  SlashCommandSpec {
    command: "/add",
    completion: "/add ",
    description: "Add an arXiv category permanently",
  },
  SlashCommandSpec {
    command: "/add-feed",
    completion: "/add-feed ",
    description: "Add an RSS feed permanently",
  },
  SlashCommandSpec {
    command: "/trending",
    completion: "/trending ",
    description: "Planned: find trending work for a topic",
  },
  SlashCommandSpec {
    command: "/watch",
    completion: "/watch ",
    description: "Planned: watch a topic over time",
  },
];

const CHAT_BG: Color = Color::Rgb(10, 14, 18);
const CHAT_PANEL_BG: Color = Color::Rgb(15, 21, 27);
const CHAT_INPUT_BG: Color = Color::Rgb(22, 31, 40);
const CHAT_ACCENT: Color = Color::Rgb(135, 206, 235);
const CHAT_HEADER: Color = Color::Rgb(62, 126, 180);
const CHAT_TEXT: Color = Color::Rgb(218, 224, 230);
const CHAT_MUTED: Color = Color::Rgb(105, 118, 130);
const CHAT_BORDER: Color = Color::Rgb(48, 60, 72);
const CHAT_SELECT_BG: Color = Color::Rgb(58, 74, 90);
const CHAT_SUCCESS: Color = Color::Rgb(132, 190, 145);
const CHAT_WARN: Color = Color::Rgb(204, 180, 105);

pub struct ChatUi {
  pub state: ChatUiState,
  pub sessions: Vec<ChatSessionMeta>,
  pub session_list_state: ListState,
  pub active_session: Option<ChatSession>,
  pub input: String,
  pub input_cursor: usize,
  pub scroll_offset: usize,
  pub provider_registry: ProviderRegistry,
  pub default_provider: String,
  pub new_session_input: String,
  pub viewport_height: usize,
  pub pending_response:
    Option<mpsc::Receiver<Result<ProviderResponse, String>>>,
  pub is_loading: bool,
  pub frame_count: u64,
  /// Words remaining to reveal during streaming simulation.
  pub streaming_words: Vec<String>,
  /// True while word-by-word reveal is in progress.
  pub is_streaming: bool,
  /// Vim-style input mode for the chat pane.
  pub input_mode: ChatInputMode,
  /// Selected row in the slash-command suggestion palette.
  pub slash_selected: usize,
}

// ── Construction ─────────────────────────────────────────────────────────────

impl ChatUi {
  pub fn new(registry: ProviderRegistry, default_provider: String) -> Self {
    let index = load_index();
    let sessions = index.sessions;
    let mut session_list_state = ListState::default();
    if !sessions.is_empty() {
      session_list_state.select(Some(0));
    }
    Self {
      state: ChatUiState::SessionList,
      sessions,
      session_list_state,
      active_session: None,
      input: String::new(),
      input_cursor: 0,
      scroll_offset: 0,
      provider_registry: registry,
      default_provider,
      new_session_input: String::new(),
      viewport_height: 20,
      pending_response: None,
      is_loading: false,
      frame_count: 0,
      streaming_words: Vec::new(),
      is_streaming: false,
      input_mode: ChatInputMode::Insert,
      slash_selected: 0,
    }
  }
}

// ── Tick (called each frame by host) ─────────────────────────────────────────

impl ChatUi {
  pub fn tick(&mut self) {
    self.frame_count = self.frame_count.wrapping_add(1);

    // Word-by-word streaming reveal: one word per tick (~16ms each).
    if self.is_streaming {
      if self.streaming_words.is_empty() {
        self.is_streaming = false;
        if let Some(session) = self.active_session.as_ref() {
          let _ = save_session(session);
          let meta = crate::storage::session_to_meta(session);
          let id = meta.id.clone();
          if let Some(pos) = self.sessions.iter().position(|s| s.id == id) {
            self.sessions[pos] = meta;
          }
          self.sync_index();
        }
      } else {
        let word = self.streaming_words.remove(0);
        if let Some(session) = self.active_session.as_mut() {
          if let Some(last_msg) = session.messages.last_mut() {
            if !last_msg.content.is_empty() {
              last_msg.content.push(' ');
            }
            last_msg.content.push_str(&word);
          }
        }
      }
      return;
    }

    if self.pending_response.is_none() {
      return;
    }

    let result = {
      let rx = self.pending_response.as_ref().unwrap();
      match rx.try_recv() {
        Ok(r) => Some(Ok(r)),
        Err(mpsc::TryRecvError::Empty) => None,
        Err(mpsc::TryRecvError::Disconnected) => {
          Some(Err("thread disconnected".to_string()))
        }
      }
    };

    match result {
      None => {}
      Some(inner) => {
        self.pending_response = None;
        self.is_loading = false;

        let response = match inner {
          Ok(Ok(resp)) => ProviderResponse {
            content: sanitize_content(&resp.content),
            ..resp
          },
          Ok(Err(e)) => ProviderResponse {
            content: parse_api_error(&e),
            input_tokens: 0,
            output_tokens: 0,
          },
          Err(e) => ProviderResponse {
            content: format!("thread error — {e}"),
            input_tokens: 0,
            output_tokens: 0,
          },
        };

        log::debug!(
          "chat: response received ({} chars, {}↑ {}↓ tokens)",
          response.content.len(),
          response.input_tokens,
          response.output_tokens
        );

        if let Some(session) = self.active_session.as_mut() {
          session.total_input_tokens += response.input_tokens;
          session.total_output_tokens += response.output_tokens;
        }

        // Split into words for streaming reveal.
        let words: Vec<String> =
          response.content.split_whitespace().map(|w| w.to_string()).collect();

        // Push a placeholder assistant message (content will fill as we stream).
        if let Some(session) = self.active_session.as_mut() {
          session.messages.push(ChatMessage {
            role: Role::Assistant,
            content: String::new(),
            timestamp: Utc::now(),
          });
          session.updated_at = Utc::now();
        }

        if words.is_empty() {
          if let Some(session) = self.active_session.as_ref() {
            let _ = save_session(session);
          }
        } else {
          self.streaming_words = words;
          self.is_streaming = true;
        }

        self.scroll_offset = usize::MAX;
      }
    }
  }
}

// ── Top-level draw / handle_key ───────────────────────────────────────────────

impl ChatUi {
  pub fn draw(&mut self, frame: &mut Frame, area: Rect) {
    match self.state {
      // Session list and new-session overlay both draw on top of the
      // chat background — always render the chat background first.
      ChatUiState::SessionList => {
        self.draw_chat_background(frame, area);
        self.draw_session_list(frame, area);
      }
      ChatUiState::NewSession => {
        self.draw_chat_background(frame, area);
        self.draw_session_list(frame, area);
        self.draw_new_session_overlay(frame, area);
      }
      ChatUiState::Chat => self.draw_chat(frame, area),
    }
  }

  /// Returns true only when the chat conversation pane is open and needs a
  /// dedicated row of screen space.  False when showing the session list or
  /// new-session overlay (those float over the main layout).
  pub fn needs_panel(&self) -> bool {
    self.state == ChatUiState::Chat
  }

  /// Render the session-list (or new-session) as a floating popup over the
  /// given area (normally the full terminal rect).  No background panel.
  pub fn draw_overlay(&mut self, frame: &mut Frame, area: Rect) {
    match self.state {
      ChatUiState::SessionList => self.draw_session_list(frame, area),
      ChatUiState::NewSession => {
        self.draw_session_list(frame, area);
        self.draw_new_session_overlay(frame, area);
      }
      _ => {}
    }
  }

  pub fn handle_key(&mut self, key: KeyEvent) -> ChatAction {
    match self.state {
      ChatUiState::SessionList => self.handle_session_list_key(key),
      ChatUiState::NewSession => self.handle_new_session_key(key),
      ChatUiState::Chat => self.handle_chat_key(key),
    }
  }
}

// ── Background ────────────────────────────────────────────────────────────────

impl ChatUi {
  fn draw_chat_background(&self, frame: &mut Frame, area: Rect) {
    let bg = Block::default().style(Style::default().bg(CHAT_BG));
    frame.render_widget(bg, area);
    // Top separator line.
    frame.render_widget(
      Paragraph::new("─".repeat(area.width as usize))
        .style(Style::default().fg(CHAT_BORDER).bg(CHAT_BG)),
      Rect { x: area.x, y: area.y, width: area.width, height: 1 },
    );
  }
}

// ── Session list ──────────────────────────────────────────────────────────────

impl ChatUi {
  fn draw_session_list(&mut self, frame: &mut Frame, area: Rect) {
    let popup_w = (area.width as u32 * 60 / 100).max(30) as u16;
    // spec: min(session_count + 4, 12)
    let popup_h = ((self.sessions.len() as u16 + 4).min(12)).max(3);
    // Centered horizontally; bottom-anchored above footer (3 rows) with 2
    // rows of clearance: y = terminal_height - popup_height - footer_height - 2
    let footer_h: u16 = 3;
    let x = area.x + area.width.saturating_sub(popup_w) / 2;
    let y =
      area.y.saturating_add(area.height.saturating_sub(popup_h + footer_h + 2));
    let popup_rect = Rect::new(
      x,
      y,
      popup_w.min(area.width),
      popup_h.min(area.height.saturating_sub(footer_h + 2)),
    );

    frame.render_widget(Clear, popup_rect);

    let items: Vec<ListItem> = self
      .sessions
      .iter()
      .map(|s| {
        let date = s.updated_at.format("%Y-%m-%d").to_string();
        let provider = s.provider.as_deref().unwrap_or("default");
        let line = Line::from(vec![
          Span::styled(
            s.title.clone(),
            Style::default().fg(CHAT_TEXT).add_modifier(Modifier::BOLD),
          ),
          Span::styled(
            format!("  {}  [{}]", date, provider),
            Style::default().fg(CHAT_MUTED),
          ),
        ]);
        ListItem::new(line)
      })
      .collect();

    let list = List::new(items)
      .block(
        Block::default()
          .borders(Borders::ALL)
          .border_style(Style::default().fg(CHAT_BORDER))
          .style(Style::default().bg(CHAT_PANEL_BG))
          .title(Span::styled(
            " ── sessions ── ",
            Style::default().fg(CHAT_HEADER),
          ))
          .title_alignment(Alignment::Center),
      )
      .highlight_style(
        Style::default().fg(CHAT_ACCENT).add_modifier(Modifier::BOLD),
      )
      .highlight_symbol("  ");

    frame.render_stateful_widget(
      list,
      popup_rect,
      &mut self.session_list_state,
    );
  }

  fn handle_session_list_key(&mut self, key: KeyEvent) -> ChatAction {
    match key.code {
      KeyCode::Esc => ChatAction::Quit,

      KeyCode::Char('n') => {
        self.new_session_input.clear();
        self.state = ChatUiState::NewSession;
        ChatAction::None
      }

      KeyCode::Enter => {
        if let Some(idx) = self.session_list_state.selected() {
          if let Some(meta) = self.sessions.get(idx) {
            let id = meta.id.clone();
            if let Some(session) = load_session(&id) {
              self.active_session = Some(session);
              self.scroll_offset = usize::MAX;
              self.input_mode = ChatInputMode::Insert;
              self.state = ChatUiState::Chat;
            }
          }
        }
        ChatAction::None
      }

      KeyCode::Char('d') => {
        if let Some(idx) = self.session_list_state.selected() {
          if idx < self.sessions.len() {
            let id = self.sessions[idx].id.clone();
            let _ = delete_session(&id);
            self.sessions.remove(idx);
            self.sync_index();
            let new_sel = if self.sessions.is_empty() {
              None
            } else {
              Some(idx.min(self.sessions.len() - 1))
            };
            self.session_list_state.select(new_sel);
          }
        }
        ChatAction::None
      }

      KeyCode::Char('j') | KeyCode::Down => {
        let len = self.sessions.len();
        if len > 0 {
          let next = self
            .session_list_state
            .selected()
            .map(|i| (i + 1).min(len - 1))
            .unwrap_or(0);
          self.session_list_state.select(Some(next));
        }
        ChatAction::None
      }

      KeyCode::Char('k') | KeyCode::Up => {
        if !self.sessions.is_empty() {
          let prev = self
            .session_list_state
            .selected()
            .map(|i| i.saturating_sub(1))
            .unwrap_or(0);
          self.session_list_state.select(Some(prev));
        }
        ChatAction::None
      }

      _ => ChatAction::None,
    }
  }
}

// ── New session overlay ───────────────────────────────────────────────────────

impl ChatUi {
  fn draw_new_session_overlay(&self, frame: &mut Frame, area: Rect) {
    let overlay = centered_rect(50, 3, area);
    frame.render_widget(Clear, overlay);

    let input_display = format!("{}_", self.new_session_input);
    let para = Paragraph::new(input_display)
      .block(
        Block::default()
          .borders(Borders::ALL)
          .border_style(Style::default().fg(CHAT_BORDER))
          .style(Style::default().bg(CHAT_PANEL_BG))
          .title(" new session (enter: confirm  esc: cancel) "),
      )
      .style(Style::default().fg(CHAT_TEXT));
    frame.render_widget(para, overlay);
  }

  fn handle_new_session_key(&mut self, key: KeyEvent) -> ChatAction {
    match key.code {
      KeyCode::Esc => {
        self.state = ChatUiState::SessionList;
        ChatAction::None
      }

      KeyCode::Enter => {
        let title = if self.new_session_input.trim().is_empty() {
          "New conversation".to_string()
        } else {
          mem::take(&mut self.new_session_input)
        };
        let session = create_session(title, None);
        let meta = crate::storage::session_to_meta(&session);
        let _ = save_session(&session);
        self.sessions.push(meta);
        self.sync_index();
        let new_idx = self.sessions.len() - 1;
        self.session_list_state.select(Some(new_idx));
        self.active_session = Some(session);
        self.input.clear();
        self.input_cursor = 0;
        self.scroll_offset = 0;
        self.input_mode = ChatInputMode::Insert;
        self.state = ChatUiState::Chat;
        ChatAction::None
      }

      KeyCode::Backspace => {
        self.new_session_input.pop();
        ChatAction::None
      }

      KeyCode::Char(c) => {
        self.new_session_input.push(c);
        ChatAction::None
      }

      _ => ChatAction::None,
    }
  }
}

// ── Chat view ─────────────────────────────────────────────────────────────────

impl ChatUi {
  fn draw_chat(&mut self, frame: &mut Frame, area: Rect) {
    let provider_name = self
      .active_session
      .as_ref()
      .and_then(|s| s.provider.as_deref().map(|p| p.to_string()))
      .unwrap_or_else(|| self.default_provider.clone());

    let model_name = self
      .provider_registry
      .get(&provider_name)
      .map(|p| p.model().to_string())
      .unwrap_or_else(|| "unknown".to_string());

    // Full background fill.
    frame.render_widget(
      Block::default().style(Style::default().bg(CHAT_BG)),
      area,
    );

    // Layout: separator(1) | header(1) | messages(fill) | input(1) | status(1)
    let chunks = Layout::default()
      .direction(Direction::Vertical)
      .constraints([
        Constraint::Length(1), // top separator
        Constraint::Length(1), // header
        Constraint::Min(0),    // message viewport
        Constraint::Length(1), // input bar
        Constraint::Length(1), // status bar
      ])
      .split(area);

    let sep_area = chunks[0];
    let header_area = chunks[1];
    let messages_area = chunks[2];
    let input_area = chunks[3];
    let status_area = chunks[4];

    // ── Top separator ──────────────────────────────────────────────
    frame.render_widget(
      Paragraph::new("─".repeat(area.width as usize))
        .style(Style::default().fg(CHAT_BORDER).bg(CHAT_BG)),
      sep_area,
    );

    // ── Header ────────────────────────────────────────────────────
    // "── chat ─── <model> · <provider> │ <session title> ──"
    let session_title = self
      .active_session
      .as_ref()
      .map(|s| s.title.clone())
      .unwrap_or_else(|| "new session".to_string());

    let prefix = "── chat ─── ";
    let mdp = format!("{model_name} · {provider_name}");
    let title_part = format!(" │ {session_title} ");
    let (mode_label, mode_color) = match self.input_mode {
      ChatInputMode::Insert => (" -- INSERT --", CHAT_SUCCESS),
      ChatInputMode::Normal => (" -- NORMAL --", CHAT_WARN),
    };
    let used =
      prefix.len() + mdp.len() + title_part.len() + mode_label.len() + 2;
    let fill = (area.width as usize).saturating_sub(used);

    let header_line = Line::from(vec![
      Span::styled(prefix.to_string(), Style::default().fg(CHAT_BORDER)),
      Span::styled(
        mdp,
        Style::default().fg(CHAT_HEADER).add_modifier(Modifier::BOLD),
      ),
      Span::styled(title_part, Style::default().fg(CHAT_MUTED)),
      Span::styled("─".repeat(fill), Style::default().fg(CHAT_BORDER)),
      Span::styled(
        mode_label.to_string(),
        Style::default().fg(mode_color).add_modifier(Modifier::BOLD),
      ),
    ]);
    frame.render_widget(
      Paragraph::new(header_line).style(Style::default().bg(CHAT_BG)),
      header_area,
    );

    // ── Messages ──────────────────────────────────────────────────
    let width = messages_area.width as usize;
    let msg_lines = self.build_message_lines(width);
    let total_lines = msg_lines.len();
    let viewport_height = messages_area.height as usize;
    self.viewport_height = viewport_height;

    let max_scroll = total_lines.saturating_sub(viewport_height);
    if self.scroll_offset > max_scroll {
      self.scroll_offset = max_scroll;
    }

    frame.render_widget(
      Paragraph::new(Text::from(msg_lines))
        .style(Style::default().bg(CHAT_BG))
        .scroll((self.scroll_offset as u16, 0)),
      messages_area,
    );

    self.draw_slash_palette(frame, messages_area);

    // Scroll indicator — top-right corner when not at bottom.
    if self.scroll_offset < max_scroll && messages_area.height > 0 {
      let label = " ↑ more ";
      let lw = label.len() as u16;
      let x = messages_area.x + messages_area.width.saturating_sub(lw);
      frame.render_widget(
        Paragraph::new(label)
          .style(Style::default().fg(CHAT_MUTED).bg(CHAT_BG)),
        Rect { x, y: messages_area.y, width: lw, height: 1 },
      );
    }

    // ── Input bar ─────────────────────────────────────────────────
    let input_bg = CHAT_INPUT_BG;
    let input_line = if self.is_loading {
      let dots_idx = ((self.frame_count / 8) as usize) % 4;
      let dots = ["·", "··", "···", "··"][dots_idx];
      Line::from(Span::styled(
        dots.to_string(),
        Style::default().fg(CHAT_MUTED).bg(input_bg),
      ))
    } else if self.input_mode == ChatInputMode::Normal {
      let text = if self.input.is_empty() {
        "Type your message or /help for commands".to_string()
      } else {
        self.input.clone()
      };
      Line::from(Span::styled(
        text,
        Style::default().fg(CHAT_MUTED).bg(input_bg),
      ))
    } else if self.input.is_empty() {
      Line::from(Span::styled(
        "Type your message or /help for commands",
        Style::default().fg(CHAT_MUTED).bg(input_bg),
      ))
    } else {
      Line::from(Span::styled(
        format!("{}█", self.input),
        Style::default().fg(CHAT_TEXT).bg(input_bg),
      ))
    };
    frame.render_widget(
      Paragraph::new(input_line).style(Style::default().bg(input_bg)),
      input_area,
    );

    // ── Status bar ────────────────────────────────────────────────
    let status_line =
      self.build_status_line(&provider_name, &model_name, area.width as usize);
    frame.render_widget(
      Paragraph::new(status_line).style(Style::default().bg(CHAT_BG)),
      status_area,
    );
  }

  fn build_status_line(
    &self,
    provider_name: &str,
    model_name: &str,
    width: usize,
  ) -> Line<'static> {
    let (in_tok, out_tok) = self
      .active_session
      .as_ref()
      .map(|s| (s.total_input_tokens, s.total_output_tokens))
      .unwrap_or((0, 0));

    if in_tok == 0 && out_tok == 0 {
      return Line::from(vec![]);
    }

    let (cost, ctx_pct, ctx_k) =
      compute_cost_and_ctx(provider_name, model_name, in_tok, out_tok);

    let s = format!(
      "↑{}  ↓{}  ${:.3}  {:.1}%/{}k (auto)  {} · {}",
      fmt_tokens(in_tok),
      fmt_tokens(out_tok),
      cost,
      ctx_pct,
      ctx_k,
      model_name,
      provider_name,
    );
    let s = if s.len() > width { s[..width].to_string() } else { s };

    Line::from(Span::styled(s, Style::default().fg(CHAT_MUTED)))
  }

  fn handle_chat_key(&mut self, key: KeyEvent) -> ChatAction {
    log::debug!(
      "chat: key event {:?} (is_loading={}, mode={:?})",
      key.code,
      self.is_loading,
      self.input_mode
    );

    // Only block keys during the actual API call, not during streaming reveal.
    if self.is_loading {
      if key.code == KeyCode::Esc {
        log::debug!("chat: request cancelled by user");
        self.pending_response = None;
        self.is_loading = false;
      }
      return ChatAction::None;
    }

    match self.input_mode {
      ChatInputMode::Insert => self.handle_chat_key_insert(key),
      ChatInputMode::Normal => self.handle_chat_key_normal(key),
    }
  }

  fn handle_chat_key_insert(&mut self, key: KeyEvent) -> ChatAction {
    match key.code {
      KeyCode::Esc => {
        self.input_mode = ChatInputMode::Normal;
        ChatAction::None
      }

      KeyCode::Enter => {
        if self.complete_slash_on_enter() {
          return ChatAction::None;
        }
        if !self.input.trim().is_empty() {
          self.send_message()
        } else {
          ChatAction::None
        }
      }

      KeyCode::Tab => {
        if self.complete_selected_slash_command() {
          return ChatAction::None;
        }
        ChatAction::None
      }

      KeyCode::Down => {
        if self.move_slash_selection(1) {
          return ChatAction::None;
        }
        ChatAction::None
      }

      KeyCode::Up => {
        if self.move_slash_selection(-1) {
          return ChatAction::None;
        }
        ChatAction::None
      }

      KeyCode::Char('n') if key.modifiers == KeyModifiers::CONTROL => {
        if self.move_slash_selection(1) {
          return ChatAction::None;
        }
        ChatAction::None
      }

      KeyCode::Char('p') if key.modifiers == KeyModifiers::CONTROL => {
        if self.move_slash_selection(-1) {
          return ChatAction::None;
        }
        ChatAction::None
      }

      KeyCode::Backspace => {
        self.input.pop();
        self.input_cursor = self.input.len();
        self.clamp_slash_selection();
        ChatAction::None
      }

      KeyCode::Char(c) => {
        self.input.push(c);
        self.input_cursor = self.input.len();
        self.clamp_slash_selection();
        ChatAction::None
      }

      _ => ChatAction::None,
    }
  }

  fn handle_chat_key_normal(&mut self, key: KeyEvent) -> ChatAction {
    match key.code {
      KeyCode::Esc => {
        self.state = ChatUiState::SessionList;
        self.input.clear();
        self.input_cursor = 0;
        self.input_mode = ChatInputMode::Insert;
        ChatAction::None
      }

      KeyCode::Char('i') | KeyCode::Char('a') => {
        self.input_mode = ChatInputMode::Insert;
        ChatAction::None
      }

      KeyCode::Enter => {
        self.input_mode = ChatInputMode::Insert;
        ChatAction::None
      }

      KeyCode::Char('j') | KeyCode::Down => {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
        ChatAction::None
      }

      KeyCode::Char('k') | KeyCode::Up => {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
        ChatAction::None
      }

      KeyCode::PageDown => {
        let step = (self.viewport_height / 2).max(1);
        self.scroll_offset = self.scroll_offset.saturating_add(step);
        ChatAction::None
      }

      KeyCode::PageUp => {
        let step = (self.viewport_height / 2).max(1);
        self.scroll_offset = self.scroll_offset.saturating_sub(step);
        ChatAction::None
      }

      _ => ChatAction::None,
    }
  }

  fn slash_suggestions(&self) -> Vec<SlashCommandSpec> {
    let input = self.input.trim_start();
    if !input.starts_with('/') {
      return Vec::new();
    }

    let query = input.to_lowercase();
    SLASH_COMMANDS
      .iter()
      .copied()
      .filter(|spec| {
        spec.command.starts_with(&query)
          || spec.completion.trim_end().starts_with(&query)
          || query.starts_with(spec.command)
          || spec.command.contains(query.trim_start_matches('/'))
      })
      .collect()
  }

  fn clamp_slash_selection(&mut self) {
    let len = self.slash_suggestions().len();
    if len == 0 {
      self.slash_selected = 0;
    } else if self.slash_selected >= len {
      self.slash_selected = len - 1;
    }
  }

  fn move_slash_selection(&mut self, delta: isize) -> bool {
    let len = self.slash_suggestions().len();
    if len == 0 {
      return false;
    }

    let current = self.slash_selected.min(len - 1) as isize;
    self.slash_selected = (current + delta).clamp(0, len as isize - 1) as usize;
    true
  }

  fn complete_selected_slash_command(&mut self) -> bool {
    let suggestions = self.slash_suggestions();
    let Some(spec) = suggestions
      .get(self.slash_selected.min(suggestions.len().saturating_sub(1)))
    else {
      return false;
    };
    self.input = spec.completion.to_string();
    self.input_cursor = self.input.len();
    true
  }

  fn complete_slash_on_enter(&mut self) -> bool {
    let input = self.input.trim();
    if !input.starts_with('/') || input.contains(' ') {
      return false;
    }

    self.complete_selected_slash_command()
  }

  fn draw_slash_palette(&mut self, frame: &mut Frame, messages_area: Rect) {
    if self.input_mode != ChatInputMode::Insert || self.is_loading {
      return;
    }

    let suggestions = self.slash_suggestions();
    if suggestions.is_empty() || messages_area.height == 0 {
      return;
    }

    self.slash_selected = self.slash_selected.min(suggestions.len() - 1);

    let height = (suggestions.len() as u16).min(6).min(messages_area.height);
    let area = Rect {
      x: messages_area.x,
      y: messages_area.y + messages_area.height.saturating_sub(height),
      width: messages_area.width,
      height,
    };

    let lines: Vec<Line> = suggestions
      .iter()
      .take(height as usize)
      .enumerate()
      .map(|(i, spec)| {
        let selected = i == self.slash_selected;
        let style = if selected {
          Style::default()
            .bg(CHAT_SELECT_BG)
            .fg(CHAT_TEXT)
            .add_modifier(Modifier::BOLD)
        } else {
          Style::default().fg(CHAT_MUTED).bg(CHAT_BG)
        };
        let command_style = if selected {
          style.fg(CHAT_TEXT)
        } else {
          Style::default().fg(CHAT_ACCENT).bg(CHAT_BG)
        };
        Line::from(vec![
          Span::styled(" ", style),
          Span::styled(
            format!("{:<20}", spec.completion.trim_end()),
            command_style,
          ),
          Span::styled(spec.description.to_string(), style),
        ])
      })
      .collect();

    frame.render_widget(
      Paragraph::new(lines).style(Style::default().bg(CHAT_BG)),
      area,
    );
  }

  fn send_message(&mut self) -> ChatAction {
    // If streaming is in progress, flush all remaining words immediately
    // so the previous message is complete before we send the next one.
    if self.is_streaming {
      let remaining = mem::take(&mut self.streaming_words);
      if let Some(session) = self.active_session.as_mut() {
        if let Some(last_msg) = session.messages.last_mut() {
          for word in &remaining {
            if !last_msg.content.is_empty() {
              last_msg.content.push(' ');
            }
            last_msg.content.push_str(word);
          }
        }
        let _ = save_session(session);
      }
      self.is_streaming = false;
    }

    let raw_input = mem::take(&mut self.input);
    self.input_cursor = 0;

    if raw_input.starts_with('/') {
      if let Some(session) = self.active_session.as_mut() {
        session.messages.push(ChatMessage {
          role: Role::User,
          content: raw_input.clone(),
          timestamp: Utc::now(),
        });
        session.updated_at = Utc::now();
        let _ = save_session(session);
        let meta = crate::storage::session_to_meta(session);
        let id = meta.id.clone();
        if let Some(pos) = self.sessions.iter().position(|s| s.id == id) {
          self.sessions[pos] = meta;
        }
        self.sync_index();
      }
      self.scroll_offset = usize::MAX;
      return ChatAction::SlashCommand(raw_input);
    }

    let (prefix, content) = parse_provider_prefix(&raw_input);
    let provider_name = prefix.unwrap_or_else(|| self.default_provider.clone());

    if let Some(session) = self.active_session.as_mut() {
      session.messages.push(ChatMessage {
        role: Role::User,
        content: content.clone(),
        timestamp: Utc::now(),
      });
      session.updated_at = Utc::now();
    }

    let messages = self
      .active_session
      .as_ref()
      .map(|s| s.messages.clone())
      .unwrap_or_default();

    let provider = match self.provider_registry.get(&provider_name) {
      Some(p) => p,
      None => {
        let err = format!("provider '{}' not registered", provider_name);
        log::debug!("chat: {err}");
        if let Some(session) = self.active_session.as_mut() {
          session.messages.push(ChatMessage {
            role: Role::Assistant,
            content: err,
            timestamp: Utc::now(),
          });
          session.updated_at = Utc::now();
          let _ = save_session(session);
          let meta = crate::storage::session_to_meta(session);
          let id = meta.id.clone();
          if let Some(pos) = self.sessions.iter().position(|s| s.id == id) {
            self.sessions[pos] = meta;
          }
          self.sync_index();
        }
        self.scroll_offset = usize::MAX;
        return ChatAction::None;
      }
    };

    log::debug!(
      "chat: spawning background thread for provider '{provider_name}'"
    );

    let (tx, rx) = mpsc::channel::<Result<ProviderResponse, String>>();
    self.pending_response = Some(rx);
    self.is_loading = true;
    self.scroll_offset = usize::MAX;

    std::thread::spawn(move || {
      let result = provider.send(&messages).map_err(|e| e.to_string());
      let _ = tx.send(result);
    });

    ChatAction::None
  }

  /// Build pre-wrapped message lines (Feynman style).
  ///
  /// User messages: full-width background highlight, white text.
  /// Assistant messages: no background, gray text, markdown bold handled.
  /// Single blank line between each pair.
  fn build_message_lines(&self, width: usize) -> Vec<Line<'static>> {
    let session = match &self.active_session {
      Some(s) => s,
      None => return vec![],
    };

    let wrap_width = width.max(1);
    let msgs: Vec<&ChatMessage> = session
      .messages
      .iter()
      .filter(|m| !matches!(m.role, Role::System))
      .collect();

    let mut lines: Vec<Line<'static>> = Vec::new();

    for (i, msg) in msgs.iter().enumerate() {
      match msg.role {
        Role::System => continue,

        Role::User => {
          let user_bg = Style::default().fg(CHAT_TEXT).bg(CHAT_SELECT_BG);
          let content = if msg.content.is_empty() {
            " ".to_string()
          } else {
            msg.content.clone()
          };
          for source_line in content.lines() {
            if source_line.is_empty() {
              lines.push(Line::from(Span::styled(
                " ".repeat(wrap_width),
                user_bg,
              )));
            } else {
              for wrapped in
                textwrap::wrap(source_line, wrap_width.saturating_sub(1).max(1))
              {
                // 1-space left padding; fill rest to full width
                let padded = format!(
                  " {:<width$}",
                  wrapped,
                  width = wrap_width.saturating_sub(1)
                );
                lines.push(Line::from(Span::styled(padded, user_bg)));
              }
            }
          }
        }

        Role::Assistant => {
          let base_style = Style::default().fg(CHAT_TEXT);
          // Show streaming cursor on the last message while streaming.
          let is_last = i + 1 == msgs.len();
          let display_content = if self.is_streaming && is_last {
            format!("{}█", msg.content)
          } else {
            msg.content.clone()
          };

          let content = if display_content.is_empty() {
            " ".to_string()
          } else {
            display_content
          };

          // No indentation — wrap at full width.
          for source_line in content.lines() {
            if source_line.trim().is_empty() {
              lines.push(Line::from(""));
            } else {
              for wrapped in textwrap::wrap(source_line, wrap_width) {
                lines.push(parse_markdown_inline(&wrapped, base_style));
              }
            }
          }
        }
      }

      // Single blank line between messages, not after the last.
      if i + 1 < msgs.len() {
        lines.push(Line::from(""));
      }
    }

    lines
  }
}

// ── Shared helpers ────────────────────────────────────────────────────────────

impl ChatUi {
  fn sync_index(&self) {
    let index = ChatIndex {
      sessions: self.sessions.clone(),
      default_provider: self.default_provider.clone(),
    };
    let _ = save_index(&index);
  }
}

/// Parse inline markdown (`**bold**`, `*italic*`) into styled spans.
/// Uses a char-indexed walk to correctly distinguish `**` from `*`.
fn parse_markdown_inline(text: &str, base_style: Style) -> Line<'static> {
  let bold_style = base_style.add_modifier(Modifier::BOLD);
  let italic_style = base_style.add_modifier(Modifier::ITALIC);

  let chars: Vec<char> = text.chars().collect();
  let mut spans: Vec<Span<'static>> = Vec::new();
  let mut i = 0;
  let mut current = String::new();
  let current_style = base_style;

  while i < chars.len() {
    if i + 1 < chars.len() && chars[i] == '*' && chars[i + 1] == '*' {
      // Look for closing `**`
      let start = i + 2;
      let mut j = start;
      while j + 1 < chars.len() && !(chars[j] == '*' && chars[j + 1] == '*') {
        j += 1;
      }
      if j + 1 < chars.len() {
        // Found closing `**`
        if !current.is_empty() {
          spans.push(Span::styled(current.clone(), current_style));
          current.clear();
        }
        let inner: String = chars[start..j].iter().collect();
        spans.push(Span::styled(inner, bold_style));
        i = j + 2;
      } else {
        current.push(chars[i]);
        i += 1;
      }
    } else if chars[i] == '*' {
      // Look for closing `*`
      let start = i + 1;
      let mut j = start;
      while j < chars.len() && chars[j] != '*' {
        j += 1;
      }
      if j < chars.len() {
        // Found closing `*`
        if !current.is_empty() {
          spans.push(Span::styled(current.clone(), current_style));
          current.clear();
        }
        let inner: String = chars[start..j].iter().collect();
        spans.push(Span::styled(inner, italic_style));
        i = j + 1;
      } else {
        current.push(chars[i]);
        i += 1;
      }
    } else {
      current.push(chars[i]);
      i += 1;
    }
  }

  if !current.is_empty() {
    spans.push(Span::styled(current, current_style));
  }

  if spans.is_empty() {
    Line::from(Span::styled(String::new(), base_style))
  } else {
    Line::from(spans)
  }
}

/// Sanitize a successful response body — if the content looks like a JSON
/// error (e.g. the provider returned a 200 with an error payload), forward it
/// through `parse_api_error` so the user sees a clean message.
fn sanitize_content(content: &str) -> String {
  let lower = content.to_lowercase();
  let looks_like_error = lower.contains("insufficient_quota")
    || lower.contains("invalid_api_key")
    || lower.contains("rate_limit")
    || (content.trim_start().starts_with('{') && lower.contains("\"error\""));
  if looks_like_error { parse_api_error(content) } else { content.to_string() }
}

/// Map a raw API error string to a friendly one-line message.
fn parse_api_error(err: &str) -> String {
  let lower = err.to_lowercase();
  if lower.contains("authentication")
    || lower.contains("invalid api key")
    || lower.contains("unauthorized")
    || lower.contains("invalid_api_key")
  {
    return "invalid API key — check settings".to_string();
  }
  if lower.contains("rate limit")
    || lower.contains("rate_limit")
    || lower.contains("429")
  {
    return "rate limit exceeded — try again shortly".to_string();
  }
  if lower.contains("quota")
    || lower.contains("insufficient_quota")
    || lower.contains("billing")
  {
    return "quota exceeded — check billing".to_string();
  }
  let short = if err.len() > 80 { &err[..80] } else { err };
  format!("API error — {short}")
}

/// Format a token count as a compact string ("1.2k", "45.3k", "1.2M").
fn fmt_tokens(n: u64) -> String {
  if n < 1_000 {
    n.to_string()
  } else if n < 1_000_000 {
    format!("{:.1}k", n as f64 / 1_000.0)
  } else {
    format!("{:.1}M", n as f64 / 1_000_000.0)
  }
}

/// Returns `(cost_usd, ctx_pct, ctx_k)` for the status bar.
fn compute_cost_and_ctx(
  provider_name: &str,
  model_name: &str,
  input_tokens: u64,
  output_tokens: u64,
) -> (f64, f64, u64) {
  let (in_rate, out_rate, ctx_window) = model_rates(provider_name, model_name);
  let cost = (input_tokens as f64 * in_rate + output_tokens as f64 * out_rate)
    / 1_000_000.0;
  let total = input_tokens + output_tokens;
  let ctx_pct = if ctx_window > 0 {
    (total as f64 / ctx_window as f64) * 100.0
  } else {
    0.0
  };
  (cost, ctx_pct, ctx_window / 1_000)
}

/// Returns `(input_$/1M, output_$/1M, context_window)` for a model.
fn model_rates(provider: &str, model: &str) -> (f64, f64, u64) {
  match provider {
    "claude" => {
      if model.contains("opus") {
        (15.00, 75.00, 200_000)
      } else if model.contains("haiku") {
        (0.80, 4.00, 200_000)
      } else {
        // sonnet (default)
        (3.00, 15.00, 200_000)
      }
    }
    "openai" => {
      if model.contains("gpt-4o") {
        (2.50, 10.00, 128_000)
      } else if model.contains("gpt-4") {
        (30.00, 60.00, 128_000)
      } else if model.contains("gpt-3.5") {
        (0.50, 1.50, 16_385)
      } else {
        (2.50, 10.00, 128_000)
      }
    }
    _ => (3.00, 15.00, 200_000),
  }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
  let x = area.x + area.width.saturating_sub(width) / 2;
  let y = area.y + area.height.saturating_sub(height) / 2;
  Rect { x, y, width: width.min(area.width), height: height.min(area.height) }
}
