use crossterm::event;

use super::{
  actions::EditorAction,
  core::{Editor, EditorMode, PendingInput},
};

impl Editor {
  pub fn handle_key(&mut self, key_event: event::KeyEvent) -> EditorAction {
    match self.handle_key_inner(key_event) {
      Ok(action) => action,
      Err(error) => {
        self.debug_log_error(&format!("handle_key failed: {error}"));
        EditorAction::NeedsRedraw
      }
    }
  }

  fn handle_key_inner(
    &mut self,
    key_event: event::KeyEvent,
  ) -> Result<EditorAction, Box<dyn std::error::Error>> {
    if self.pending_input.is_some() {
      self.consume_pending_input(key_event);
      return Ok(self.action_from_redraw_state());
    }

    let active_mode = self.get_active_mode();
    self.debug_log(&format!(
      "=== handle_event: key={:?}, active_buffer={}, active_mode={:?}, view_mode={:?} ===",
      key_event, self.active_buffer, active_mode, self.view_mode
    ));
    // Settings popup intercepts all keys while open
    if self.show_settings {
      if let Some(result) = self.handle_settings_key(key_event)? {
        return Ok(if result {
          EditorAction::Quit
        } else {
          self.action_from_redraw_state()
        });
      }
    }

    // Route to mode-specific handlers first
    let result = match active_mode {
      EditorMode::Normal => self.handle_normal_mode_event(key_event),
      EditorMode::VisualChar | EditorMode::VisualLine => {
        self.handle_visual_mode_event(key_event)
      }
      EditorMode::Search | EditorMode::ReverseSearch => {
        self.handle_search_mode_event(key_event)
      }
      EditorMode::Command | EditorMode::CommandExecution => {
        self.handle_command_mode_event(key_event)
      }
      EditorMode::Tutorial => Ok(self.process_tutorial_key(key_event.code)),
    };

    // Process tutorial key after mode-specific handling, only if in tutorial
    // mode
    if self.tutorial_active {
      // Don't process tutorial keys if we're in a special input mode
      match self.get_active_mode() {
        EditorMode::Command
        | EditorMode::CommandExecution
        | EditorMode::Search
        | EditorMode::ReverseSearch => {
          // Skip tutorial processing for input modes
        }
        _ => {
          // Process tutorial key for other modes
          self.process_tutorial_key(key_event.code);
        }
      }
    }

    if result? {
      Ok(EditorAction::Quit)
    } else {
      Ok(self.action_from_redraw_state())
    }
  }

  fn action_from_redraw_state(&self) -> EditorAction {
    if self.needs_redraw {
      EditorAction::NeedsRedraw
    } else {
      EditorAction::None
    }
  }

  pub fn begin_pending_input(
    &mut self,
    pending_input: PendingInput,
  ) -> Result<(), Box<dyn std::error::Error>> {
    self.pending_input = Some(pending_input);
    if self.tutorial_demo_mode
      && let Some(next_key) = self.check_demo_progress()
    {
      self.consume_pending_input(next_key);
    }
    Ok(())
  }

  fn consume_pending_input(&mut self, key_event: event::KeyEvent) {
    let Some(pending_input) = self.pending_input.take() else {
      return;
    };

    match pending_input {
      PendingInput::CommandRegister => self.handle_pending_register(key_event),
      PendingInput::CharFind { forward, till, visual } => {
        self.handle_pending_char_find(key_event, forward, till, visual)
      }
      PendingInput::GotoPrefix => self.handle_pending_goto(key_event),
      PendingInput::SetMark => self.handle_pending_set_mark(key_event),
      PendingInput::JumpToMark => self.handle_pending_mark_jump(key_event),
      PendingInput::VisualTextObject { around } => {
        self.handle_pending_visual_text_object(key_event, around)
      }
      PendingInput::OperatorTextObjectInner => {
        self.handle_pending_inner_text_object(key_event)
      }
    }
  }

  fn handle_pending_register(&mut self, key_event: event::KeyEvent) {
    if key_event.code != event::KeyCode::Char('0') {
      return;
    }

    let pos = self.editor_state.command_cursor_pos;
    let yank_text = self.editor_state.yank_buffer.clone();
    let clean_text = yank_text.replace('\n', " ").replace('\r', "");
    self.editor_state.command_buffer.insert_str(pos, &clean_text);
    self.editor_state.command_cursor_pos += clean_text.len();
    if let Some(buffer) = self.buffers.get_mut(self.active_buffer) {
      buffer.command_buffer.insert_str(pos, &clean_text);
      buffer.command_cursor_pos = self.editor_state.command_cursor_pos;
    }
    if self.tutorial_active {
      self.tutorial_paste_performed = true;
      self.debug_log("Tutorial: paste performed via Ctrl+R 0");
    }
    self.mark_dirty();
  }

  fn handle_pending_char_find(
    &mut self,
    key_event: event::KeyEvent,
    forward: bool,
    till: bool,
    visual: bool,
  ) {
    if let event::KeyCode::Char(c) = key_event.code
      && let Some(pos) = self.find_char_on_line(c, forward, till)
    {
      self.cursor_x = pos;
      if visual {
        self.update_selection();
      } else {
        self.last_find_char = Some(c);
        self.last_find_forward = forward;
        self.last_find_till = till;
      }
      self.mark_dirty();
    }
  }

  fn handle_pending_goto(&mut self, key_event: event::KeyEvent) {
    match key_event.code {
      event::KeyCode::Char('g') => self.goto_line_with_overscroll(0),
      event::KeyCode::Char('v') => self.restore_visual_selection(),
      _ => {}
    }
  }

  fn handle_pending_set_mark(&mut self, key_event: event::KeyEvent) {
    if let event::KeyCode::Char(mark_char) = key_event.code
      && mark_char.is_ascii_lowercase()
    {
      let (line, col) = self.get_cursor_position();
      self.marks.insert(mark_char, (line, col));
      self.save_bookmarks();
      self.mark_dirty();
    }
  }

  fn handle_pending_mark_jump(&mut self, key_event: event::KeyEvent) {
    match key_event.code {
      event::KeyCode::Char('\'') => {
        if let Some((line, col)) = self.previous_position {
          let current_pos = self.get_cursor_position();
          self.previous_position = Some(current_pos);
          self.move_to_position(line, col);
        }
      }
      event::KeyCode::Char(mark_char) if mark_char.is_ascii_lowercase() => {
        if let Some(&(line, col)) = self.marks.get(&mark_char) {
          let current_pos = self.get_cursor_position();
          self.previous_position = Some(current_pos);
          self.move_to_position(line, col);
          if self.tutorial_active {
            self.tutorial_bookmark_jumped = true;
          }
        }
      }
      _ => {}
    }
  }

  fn handle_pending_visual_text_object(
    &mut self,
    key_event: event::KeyEvent,
    around: bool,
  ) {
    match (around, key_event.code) {
      (false, event::KeyCode::Char('{') | event::KeyCode::Char('}')) => {
        self.select_inner_braces();
      }
      (false, event::KeyCode::Char('(') | event::KeyCode::Char(')')) => {
        self.select_inner_parentheses();
      }
      (false, event::KeyCode::Char('[') | event::KeyCode::Char(']')) => {
        self.select_inner_brackets();
      }
      (false, event::KeyCode::Char('"')) => self.select_inner_quotes('"'),
      (false, event::KeyCode::Char('\'')) => self.select_inner_quotes('\''),
      (false, event::KeyCode::Char('p')) => self.select_inner_paragraph(),
      (false, event::KeyCode::Char('s')) => self.select_inner_sentence(),
      (false, event::KeyCode::Char('w')) => self.select_inner_word(false),
      (false, event::KeyCode::Char('W')) => self.select_inner_word(true),
      (true, event::KeyCode::Char('{') | event::KeyCode::Char('}')) => {
        self.select_around_braces();
      }
      (true, event::KeyCode::Char('(') | event::KeyCode::Char(')')) => {
        self.select_around_parentheses();
      }
      (true, event::KeyCode::Char('[') | event::KeyCode::Char(']')) => {
        self.select_around_brackets();
      }
      (true, event::KeyCode::Char('"')) => self.select_around_quotes('"'),
      (true, event::KeyCode::Char('\'')) => self.select_around_quotes('\''),
      (true, event::KeyCode::Char('p')) => self.select_around_paragraph(),
      (true, event::KeyCode::Char('s')) => self.select_around_sentence(),
      (true, event::KeyCode::Char('w')) => self.select_around_word(false),
      (true, event::KeyCode::Char('W')) => self.select_around_word(true),
      _ => {}
    }
  }

  fn handle_pending_inner_text_object(&mut self, key_event: event::KeyEvent) {
    match key_event.code {
      event::KeyCode::Char('w') => {
        self.select_inner_word(false);
      }
      event::KeyCode::Char('W') => {
        self.select_inner_word(true);
      }
      event::KeyCode::Char('"')
      | event::KeyCode::Char('\'')
      | event::KeyCode::Char('(')
      | event::KeyCode::Char(')')
      | event::KeyCode::Char('{')
      | event::KeyCode::Char('}')
      | event::KeyCode::Char('[')
      | event::KeyCode::Char(']') => {
        if let event::KeyCode::Char(c) = key_event.code
          && let Some((start, end)) = self.find_text_object(c)
        {
          let line_idx = self.offset + self.cursor_y;
          self.editor_state.selection_start = Some((line_idx, start));
          self.editor_state.selection_end = Some((line_idx, end));
          self.mark_dirty();
        }
      }
      _ => {}
    }
    self.editor_state.operator_pending = None;
  }
}
