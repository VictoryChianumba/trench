use std::mem;
use std::sync::mpsc;

use chrono::Utc;
use crossterm::event::{KeyCode, KeyEvent};
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
    pub viewport_height: usize,
    pub pending_response: Option<mpsc::Receiver<Result<ProviderResponse, String>>>,
    pub is_loading: bool,
    pub frame_count: u64,
    /// Words remaining to reveal during streaming simulation.
    pub streaming_words: Vec<String>,
    /// True while word-by-word reveal is in progress.
    pub is_streaming: bool,
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
                    Ok(Ok(resp)) => resp,
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
                let words: Vec<String> = response
                    .content
                    .split_whitespace()
                    .map(|w| w.to_string())
                    .collect();

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
        let bg = Block::default().style(Style::default().bg(Color::Rgb(20, 20, 20)));
        frame.render_widget(bg, area);
        // Top separator line.
        frame.render_widget(
            Paragraph::new("─".repeat(area.width as usize))
                .style(Style::default().fg(Color::DarkGray).bg(Color::Rgb(20, 20, 20))),
            Rect { x: area.x, y: area.y, width: area.width, height: 1 },
        );
    }
}

// ── Session list ──────────────────────────────────────────────────────────────

impl ChatUi {
    fn draw_session_list(&mut self, frame: &mut Frame, area: Rect) {
        let popup_w = (area.width as u32 * 60 / 100).max(30) as u16;
        let popup_h = ((self.sessions.len() + 2) as u16).min(12).max(5);
        let hint_h: u16 = 1;
        let x = area.x + area.width.saturating_sub(popup_w) / 2;
        // Anchor to bottom of chat area, with one row below for hints.
        let y = area
            .y
            .saturating_add(area.height.saturating_sub(popup_h + hint_h + 1));
        let popup_rect = Rect::new(
            x,
            y,
            popup_w.min(area.width),
            popup_h.min(area.height.saturating_sub(hint_h + 1)),
        );
        let hint_rect = Rect::new(
            x,
            popup_rect.y + popup_rect.height,
            popup_w.min(area.width),
            hint_h,
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
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("  {}  [{}]", date, provider),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]);
                ListItem::new(line)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray))
                    .style(Style::default().bg(Color::Rgb(22, 22, 22)))
                    .title(Span::styled(
                        " ── sessions ── ",
                        Style::default().fg(Color::DarkGray),
                    ))
                    .title_alignment(Alignment::Center),
            )
            .highlight_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("  ");

        frame.render_stateful_widget(list, popup_rect, &mut self.session_list_state);

        // Keybinding hints below the popup (not inside).
        frame.render_widget(
            Paragraph::new("n: new  enter: open  d: delete  esc: close")
                .style(Style::default().fg(Color::DarkGray).bg(Color::Rgb(20, 20, 20))),
            hint_rect,
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
                    .border_style(Style::default().fg(Color::DarkGray))
                    .style(Style::default().bg(Color::Rgb(22, 22, 22)))
                    .title(" new session (enter: confirm  esc: cancel) "),
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

        let model_name = self
            .provider_registry
            .get(&provider_name)
            .map(|p| p.model().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        // Full background fill.
        frame.render_widget(
            Block::default().style(Style::default().bg(Color::Rgb(20, 20, 20))),
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

        let sep_area      = chunks[0];
        let header_area   = chunks[1];
        let messages_area = chunks[2];
        let input_area    = chunks[3];
        let status_area   = chunks[4];

        // ── Top separator ──────────────────────────────────────────────
        frame.render_widget(
            Paragraph::new("─".repeat(area.width as usize))
                .style(Style::default().fg(Color::DarkGray).bg(Color::Rgb(20, 20, 20))),
            sep_area,
        );

        // ── Header ────────────────────────────────────────────────────
        // "── chat ─── <model> · <provider> │ <session title> ──"
        let session_title = self
            .active_session
            .as_ref()
            .map(|s| s.title.clone())
            .unwrap_or_else(|| "new session".to_string());

        let prefix     = "── chat ─── ";
        let mdp        = format!("{model_name} · {provider_name}");
        let title_part = format!(" │ {session_title} ");
        let used = prefix.len() + mdp.len() + title_part.len() + 2; // +2 for trailing "──"
        let fill = (area.width as usize).saturating_sub(used);

        let header_line = Line::from(vec![
            Span::styled(prefix.to_string(), Style::default().fg(Color::DarkGray)),
            Span::styled(
                mdp,
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(title_part, Style::default().fg(Color::DarkGray)),
            Span::styled("─".repeat(fill), Style::default().fg(Color::DarkGray)),
        ]);
        frame.render_widget(
            Paragraph::new(header_line)
                .style(Style::default().bg(Color::Rgb(20, 20, 20))),
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
                .style(Style::default().bg(Color::Rgb(20, 20, 20)))
                .scroll((self.scroll_offset as u16, 0)),
            messages_area,
        );

        // Scroll indicator — top-right corner when not at bottom.
        if self.scroll_offset < max_scroll && messages_area.height > 0 {
            let label = " ↑ more ";
            let lw = label.len() as u16;
            let x = messages_area.x + messages_area.width.saturating_sub(lw);
            frame.render_widget(
                Paragraph::new(label).style(
                    Style::default()
                        .fg(Color::DarkGray)
                        .bg(Color::Rgb(20, 20, 20)),
                ),
                Rect { x, y: messages_area.y, width: lw, height: 1 },
            );
        }

        // ── Input bar ─────────────────────────────────────────────────
        let input_bg = Color::Rgb(28, 28, 28);
        let input_line = if self.is_loading {
            let dots_idx = ((self.frame_count / 8) as usize) % 4;
            let dots = ["·", "··", "···", "··"][dots_idx];
            Line::from(vec![
                Span::styled("❯ ", Style::default().fg(Color::Cyan).bg(input_bg)),
                Span::styled(
                    dots.to_string(),
                    Style::default().fg(Color::DarkGray).bg(input_bg),
                ),
            ])
        } else if self.input.is_empty() {
            Line::from(vec![
                Span::styled("❯ ", Style::default().fg(Color::Cyan).bg(input_bg)),
                Span::styled(
                    "Type your message...",
                    Style::default().fg(Color::DarkGray).bg(input_bg),
                ),
            ])
        } else {
            Line::from(vec![
                Span::styled("❯ ", Style::default().fg(Color::Cyan).bg(input_bg)),
                Span::styled(
                    format!("{}_", self.input),
                    Style::default().fg(Color::White).bg(input_bg),
                ),
            ])
        };
        frame.render_widget(
            Paragraph::new(input_line).style(Style::default().bg(input_bg)),
            input_area,
        );

        // ── Status bar ────────────────────────────────────────────────
        let status_line =
            self.build_status_line(&provider_name, &model_name, area.width as usize);
        frame.render_widget(
            Paragraph::new(status_line)
                .style(Style::default().bg(Color::Rgb(20, 20, 20))),
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
            return Line::from(Span::styled(
                "enter: send  esc: back  pgup/pgdn j/k: scroll  Ldr+z: move panel",
                Style::default().fg(Color::DarkGray),
            ));
        }

        let (cost, ctx_pct, ctx_k) =
            compute_cost_and_ctx(provider_name, model_name, in_tok, out_tok);

        let s = format!(
            "↑{}  ↓{}  ${:.3}  {:.1}%/{}k  {} · {}",
            fmt_tokens(in_tok),
            fmt_tokens(out_tok),
            cost,
            ctx_pct,
            ctx_k,
            model_name,
            provider_name,
        );
        let s = if s.len() > width {
            s[..width].to_string()
        } else {
            s
        };

        Line::from(Span::styled(s, Style::default().fg(Color::DarkGray)))
    }

    fn handle_chat_key(&mut self, key: KeyEvent) -> ChatAction {
        log::debug!("chat: key event {:?} (is_loading={})", key.code, self.is_loading);

        // Only block keys during the actual API call, not during streaming reveal.
        if self.is_loading {
            if key.code == KeyCode::Esc {
                log::debug!("chat: request cancelled by user");
                self.pending_response = None;
                self.is_loading = false;
            }
            return ChatAction::None;
        }

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

            KeyCode::PageUp => {
                let step = (self.viewport_height / 2).max(1);
                self.scroll_offset = self.scroll_offset.saturating_sub(step);
                ChatAction::None
            }

            KeyCode::PageDown => {
                let step = (self.viewport_height / 2).max(1);
                self.scroll_offset = self.scroll_offset.saturating_add(step);
                ChatAction::None
            }

            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
                ChatAction::None
            }

            KeyCode::Char('j') | KeyCode::Down => {
                self.scroll_offset = self.scroll_offset.saturating_add(1);
                ChatAction::None
            }

            KeyCode::Char(c) => {
                self.input.push(c);
                self.input_cursor = self.input.len();
                ChatAction::None
            }

            _ => ChatAction::None,
        }
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

        log::debug!("chat: spawning background thread for provider '{provider_name}'");

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
                    let user_bg =
                        Style::default().fg(Color::White).bg(Color::Rgb(35, 35, 35));
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
                            for wrapped in textwrap::wrap(source_line, wrap_width) {
                                let padded =
                                    format!("{:<width$}", wrapped, width = wrap_width);
                                lines.push(Line::from(Span::styled(padded, user_bg)));
                            }
                        }
                    }
                }

                Role::Assistant => {
                    let base_style = Style::default().fg(Color::Gray);
                    // Show streaming cursor on the last message while streaming.
                    let is_last = i + 1 == msgs.len();
                    let display_content = if self.is_streaming && is_last {
                        format!("{}█", msg.content)
                    } else {
                        msg.content.clone()
                    };

                    let content = if display_content.is_empty() {
                        "  ".to_string()
                    } else {
                        display_content
                    };

                    // Indent by 2 spaces; wrap at (width - 2).
                    let inner_wrap = wrap_width.saturating_sub(2).max(1);
                    for source_line in content.lines() {
                        if source_line.trim().is_empty() {
                            lines.push(Line::from(Span::styled(
                                "  ".to_string(),
                                base_style,
                            )));
                        } else {
                            for wrapped in textwrap::wrap(source_line, inner_wrap) {
                                lines.push(parse_markdown_bold(
                                    &format!("  {wrapped}"),
                                    base_style,
                                ));
                            }
                        }
                    }
                }
            }

            // Single blank line between messages, not after the last.
            if i + 1 < msgs.len() {
                lines.push(Line::from(Span::styled(
                    " ".repeat(wrap_width),
                    Style::default().bg(Color::Rgb(20, 20, 20)),
                )));
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

/// Parse `**bold**` markdown into styled spans.  Handles inline bold within a
/// single line; ignores unmatched `**`.
fn parse_markdown_bold(text: &str, base_style: Style) -> Line<'static> {
    let bold_style = base_style.add_modifier(Modifier::BOLD);
    let parts: Vec<&str> = text.split("**").collect();
    // Need at least 3 parts for one bold segment ("before", "bold", "after").
    if parts.len() < 3 || parts.len() % 2 == 0 {
        return Line::from(Span::styled(text.to_string(), base_style));
    }
    let spans: Vec<Span<'static>> = parts
        .iter()
        .enumerate()
        .map(|(i, part)| {
            if i % 2 == 1 {
                Span::styled(part.to_string(), bold_style)
            } else {
                Span::styled(part.to_string(), base_style)
            }
        })
        .collect();
    Line::from(spans)
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
    if lower.contains("rate limit") || lower.contains("rate_limit") || lower.contains("429") {
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
    let cost =
        (input_tokens as f64 * in_rate + output_tokens as f64 * out_rate) / 1_000_000.0;
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
    Rect {
        x,
        y,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}
