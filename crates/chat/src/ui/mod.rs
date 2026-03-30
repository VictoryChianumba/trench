use std::mem;

use chrono::Utc;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::{
    ChatIndex, ChatMessage, ChatSession, ChatSessionMeta, Role,
    provider_registry::{parse_provider_prefix, ProviderRegistry},
    storage::{
        create_session, delete_session, load_index, load_session,
        save_index, save_session,
    },
};

// ── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatUiState {
    SessionList,
    Chat,
    NewSession,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatAction {
    None,
    Quit,
    Sending,
}

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
        }
    }
}

// ── Top-level draw / handle_key ───────────────────────────────────────────────

impl ChatUi {
    pub fn draw(&mut self, frame: &mut Frame, area: Rect) {
        match self.state {
            ChatUiState::SessionList => self.draw_session_list(frame, area),
            ChatUiState::NewSession => {
                self.draw_session_list(frame, area);
                self.draw_new_session_overlay(frame, area);
            }
            ChatUiState::Chat => self.draw_chat(frame, area),
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

// ── Session list ──────────────────────────────────────────────────────────────

impl ChatUi {
    fn draw_session_list(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);

        let items: Vec<ListItem> = self
            .sessions
            .iter()
            .map(|s| {
                let date = s.updated_at.format("%Y-%m-%d %H:%M").to_string();
                let provider = s
                    .provider
                    .as_deref()
                    .unwrap_or("default");
                let line = Line::from(vec![
                    Span::styled(
                        s.title.clone(),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(format!("  {date}  [{provider}]")),
                ]);
                ListItem::new(line)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Chat Sessions "),
            )
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        frame.render_stateful_widget(list, chunks[0], &mut self.session_list_state);

        let footer = Paragraph::new(
            "n: new  enter: open  d: delete  esc: quit",
        )
        .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(footer, chunks[1]);
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
                            self.scroll_offset = usize::MAX; // scroll to bottom
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
                        // Adjust selection
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
        let overlay = centered_rect(50, 5, area);
        frame.render_widget(Clear, overlay);

        let input_display = format!("{}_", self.new_session_input);
        let para = Paragraph::new(input_display)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" New session name (Enter to confirm, Esc to cancel) "),
            )
            .style(Style::default().fg(Color::White));
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

        let title = self
            .active_session
            .as_ref()
            .map(|s| format!(" {} [{}] ", s.title, provider_name))
            .unwrap_or_else(|| " Chat ".to_string());

        // Layout: header(1) | messages(fill) | input(3) | footer(1)
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(3),
                Constraint::Length(1),
            ])
            .split(area);

        let header_area = chunks[0];
        let messages_area = chunks[1];
        let input_area = chunks[2];
        let footer_area = chunks[3];

        // Header
        let header = Paragraph::new(title.clone())
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        frame.render_widget(header, header_area);

        // Messages
        let msg_text = self.build_message_text(&provider_name);
        let total_lines = self.count_message_lines();
        let visible = messages_area.height as usize;
        let max_scroll = total_lines.saturating_sub(visible);
        if self.scroll_offset > max_scroll {
            self.scroll_offset = max_scroll;
        }
        let scroll = self.scroll_offset as u16;

        let messages_para = Paragraph::new(msg_text)
            .block(Block::default().borders(Borders::ALL))
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0));
        frame.render_widget(messages_para, messages_area);

        // Input
        let input_display = format!("{}_", self.input);
        let input_para = Paragraph::new(input_display)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Message "),
            )
            .style(Style::default().fg(Color::White));
        frame.render_widget(input_para, input_area);

        // Footer
        let footer = Paragraph::new(
            "enter: send  esc: back  pgup/pgdn: scroll",
        )
        .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(footer, footer_area);
    }

    fn handle_chat_key(&mut self, key: KeyEvent) -> ChatAction {
        match key.code {
            KeyCode::Esc => {
                self.state = ChatUiState::SessionList;
                self.input.clear();
                self.input_cursor = 0;
                ChatAction::None
            }

            KeyCode::Enter => {
                if !self.input.trim().is_empty() {
                    self.send_message()
                } else {
                    ChatAction::None
                }
            }

            KeyCode::Backspace => {
                self.input.pop();
                self.input_cursor = self.input.len();
                ChatAction::None
            }

            KeyCode::Char(c) => {
                self.input.push(c);
                self.input_cursor = self.input.len();
                ChatAction::None
            }

            KeyCode::PageUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(5);
                ChatAction::None
            }

            KeyCode::PageDown => {
                self.scroll_offset = self.scroll_offset.saturating_add(5);
                ChatAction::None
            }

            _ => ChatAction::None,
        }
    }

    fn send_message(&mut self) -> ChatAction {
        let raw_input = mem::take(&mut self.input);
        self.input_cursor = 0;

        let (prefix, content) = parse_provider_prefix(&raw_input);
        let provider_name =
            prefix.unwrap_or_else(|| self.default_provider.clone());

        // Add user message
        if let Some(session) = self.active_session.as_mut() {
            session.messages.push(ChatMessage {
                role: Role::User,
                content: content.clone(),
                timestamp: Utc::now(),
            });
            session.updated_at = Utc::now();
        }

        // Clone messages for the API call (releases the mutable borrow)
        let messages = self
            .active_session
            .as_ref()
            .map(|s| s.messages.clone())
            .unwrap_or_default();

        // Call provider synchronously
        let response_text = match self.provider_registry.get(&provider_name) {
            Some(provider) => match provider.send(&messages) {
                Ok(text) => text,
                Err(e) => format!("[error] {e}"),
            },
            None => format!(
                "[error] provider '{}' not registered",
                provider_name
            ),
        };

        // Add assistant message and persist
        if let Some(session) = self.active_session.as_mut() {
            session.messages.push(ChatMessage {
                role: Role::Assistant,
                content: response_text,
                timestamp: Utc::now(),
            });
            session.updated_at = Utc::now();

            let _ = save_session(session);

            // Update in-memory sessions list and index
            let meta = crate::storage::session_to_meta(session);
            let id = meta.id.clone();
            if let Some(pos) = self.sessions.iter().position(|s| s.id == id) {
                self.sessions[pos] = meta;
            }
            self.sync_index();
        }

        self.scroll_offset = usize::MAX; // clamped to bottom in draw_chat
        ChatAction::None
    }

    fn build_message_text(&self, provider_name: &str) -> Text<'static> {
        let session = match &self.active_session {
            Some(s) => s,
            None => return Text::default(),
        };

        let mut lines: Vec<Line<'static>> = Vec::new();
        for msg in &session.messages {
            let (label, label_style) = match msg.role {
                Role::System => continue,
                Role::User => (
                    "[you]".to_string(),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
                Role::Assistant => (
                    format!("[{provider_name}]"),
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                ),
            };

            lines.push(Line::from(Span::styled(label, label_style)));
            for content_line in msg.content.lines() {
                lines.push(Line::from(content_line.to_string()));
            }
            lines.push(Line::from(""));
        }

        Text::from(lines)
    }

    fn count_message_lines(&self) -> usize {
        let session = match &self.active_session {
            Some(s) => s,
            None => return 0,
        };
        let mut count = 0;
        for msg in &session.messages {
            if matches!(msg.role, Role::System) {
                continue;
            }
            count += 1; // role label line
            count += msg.content.lines().count().max(1);
            count += 1; // blank separator
        }
        count
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

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect {
        x,
        y,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}
