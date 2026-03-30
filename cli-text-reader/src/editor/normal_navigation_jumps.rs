use super::core::{Editor, PendingInput};
use crossterm::event::KeyCode;

impl Editor {
  // Handle jump/goto keys (g/G/0/$^/%)
  pub fn handle_jump_keys(
    &mut self,
    key_code: KeyCode,
  ) -> Result<Option<bool>, Box<dyn std::error::Error>> {
    match key_code {
      KeyCode::Char('g') => {
        self.begin_pending_input(PendingInput::GotoPrefix)?;
        Ok(Some(false))
      }
      KeyCode::Char('G') => {
        // 'G' - go to last line or specific line number with overscroll
        if self.number_prefix.is_empty() {
          // No number prefix - go to last line
          let last_line = self.total_lines.saturating_sub(1);
          self.goto_line_with_overscroll(last_line);
        } else {
          // Number prefix - go to specific line
          if let Ok(line_num) = self.number_prefix.parse::<usize>() {
            let target_line = if line_num > 0 { line_num - 1 } else { 0 }; // Convert to 0-based
            let target_line =
              target_line.min(self.total_lines.saturating_sub(1));
            self.goto_line_with_overscroll(target_line);
          }
          self.number_prefix.clear();
        }
        Ok(Some(false))
      }
      KeyCode::Char('0') => {
        // '0' - jump to start of line
        self.cursor_x = 0;
        Ok(Some(false))
      }
      KeyCode::Char('$') => {
        // '$' - jump to end of line
        let current_line_idx = self.offset + self.cursor_y;
        if current_line_idx < self.lines.len() {
          let line_length = self.lines[current_line_idx].len();
          self.cursor_x = if line_length > 0 { line_length - 1 } else { 0 };
        }
        Ok(Some(false))
      }
      KeyCode::Char('^') => {
        // '^' - jump to first non-whitespace character
        let current_line_idx = self.offset + self.cursor_y;
        if current_line_idx < self.lines.len() {
          let line = &self.lines[current_line_idx];
          for (idx, c) in line.char_indices() {
            if !c.is_whitespace() {
              self.cursor_x = idx;
              return Ok(Some(false));
            }
          }
          // If line is all whitespace, go to start
          self.cursor_x = 0;
        }
        Ok(Some(false))
      }
      KeyCode::Char('%') => {
        // Jump to matching bracket/parenthesis
        if let Some((line, col)) = self.find_matching_bracket() {
          self.move_to_position(line, col);
        }
        Ok(Some(false))
      }
      _ => Ok(None),
    }
  }

  // Handle text object navigation ({}()HML)
  pub fn handle_text_object_keys(
    &mut self,
    key_code: KeyCode,
  ) -> Result<Option<bool>, Box<dyn std::error::Error>> {
    match key_code {
      KeyCode::Char('{') => {
        // Previous paragraph
        let (line, col) = self.find_paragraph_boundary(false);
        self.move_to_position(line, col);
        Ok(Some(false))
      }
      KeyCode::Char('}') => {
        // Next paragraph
        let (line, col) = self.find_paragraph_boundary(true);
        self.move_to_position(line, col);
        Ok(Some(false))
      }
      KeyCode::Char('(') => {
        // Previous sentence
        let (line, col) = self.find_sentence_boundary(false);
        self.move_to_position(line, col);
        Ok(Some(false))
      }
      KeyCode::Char(')') => {
        // Next sentence
        let (line, col) = self.find_sentence_boundary(true);
        self.move_to_position(line, col);
        Ok(Some(false))
      }
      KeyCode::Char('H') => {
        // High - top of screen
        let (line, col) = self.get_screen_position('H');
        self.move_to_position(line, col);
        Ok(Some(false))
      }
      KeyCode::Char('M') => {
        // Middle - middle of screen
        let (line, col) = self.get_screen_position('M');
        self.move_to_position(line, col);
        Ok(Some(false))
      }
      KeyCode::Char('L') => {
        // Low - bottom of screen
        let (line, col) = self.get_screen_position('L');
        self.move_to_position(line, col);
        Ok(Some(false))
      }
      _ => Ok(None),
    }
  }

  // Handle marks and bookmarks (m/')
  pub fn handle_mark_keys(
    &mut self,
    key_code: KeyCode,
  ) -> Result<Option<bool>, Box<dyn std::error::Error>> {
    match key_code {
      KeyCode::Char('m') => {
        self.begin_pending_input(PendingInput::SetMark)?;
        Ok(Some(false))
      }
      KeyCode::Char('\'') => {
        self.begin_pending_input(PendingInput::JumpToMark)?;
        Ok(Some(false))
      }
      _ => Ok(None),
    }
  }
}
