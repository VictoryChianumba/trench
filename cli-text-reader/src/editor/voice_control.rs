use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::core::Editor;
use crate::voice::PlaybackStatus;

impl Editor {
  // -----------------------------------------------------------------------
  // Voice key handler — called from handle_normal_mode_event
  // -----------------------------------------------------------------------

  pub fn handle_voice_keys(
    &mut self,
    key_event: KeyEvent,
  ) -> Result<Option<bool>, Box<dyn std::error::Error>> {
    match key_event.code {
      // r — enter reading mode (if not already), or re/read current paragraph
      KeyCode::Char('r') => {
        if !self.reading_mode {
          self.reading_mode = true;
          self.mark_dirty();
        } else {
          // Already in reading mode — stop any current playback and re-read
          if let Some(vc) = &self.voice_controller {
            if !matches!(self.voice_status, PlaybackStatus::Idle) {
              vc.stop();
            }
          }
          self.continuous_reading = false;
          let (text, start, end) = self.get_current_paragraph_with_lines();
          if !text.is_empty() {
            self.voice_start(text, start, end);
            self.mark_dirty();
          }
        }
        Ok(Some(false))
      }

      // R — silently enter reading mode and read cursor → end of current paragraph
      KeyCode::Char('R') => {
        self.reading_mode = true;
        self.continuous_reading = false;
        if let Some(vc) = &self.voice_controller {
          if !matches!(self.voice_status, PlaybackStatus::Idle) {
            vc.stop();
          }
        }
        let (text, start, end) =
          self.get_text_from_cursor_to_end_of_paragraph_with_lines();
        if !text.is_empty() {
          self.voice_start(text, start, end);
          self.mark_dirty();
        }
        Ok(Some(false))
      }

      // Ctrl+p — start continuous reading from cursor to end of document
      KeyCode::Char('p')
        if self.reading_mode
          && key_event.modifiers.contains(KeyModifiers::CONTROL) =>
      {
        self.continuous_reading = true;
        if let Some(vc) = &self.voice_controller {
          if !matches!(self.voice_status, PlaybackStatus::Idle) {
            vc.stop();
          }
        }
        let (text, start, end) = self.get_current_paragraph_with_lines();
        if !text.is_empty() {
          self.voice_start(text, start, end);
          self.mark_dirty();
        }
        Ok(Some(false))
      }

      // Space — pause / resume (only in reading mode; fall through otherwise)
      KeyCode::Char(' ') if self.reading_mode => {
        self.sync_voice_status();
        match self.voice_status {
          PlaybackStatus::Playing => {
            if let Some(vc) = &self.voice_controller {
              vc.pause();
            }
            self.mark_dirty();
          }
          PlaybackStatus::Paused => {
            if let Some(vc) = &self.voice_controller {
              vc.resume();
            }
            self.mark_dirty();
          }
          PlaybackStatus::Loading | PlaybackStatus::Idle => {}
        }
        Ok(Some(false))
      }

      // c — re-centre viewport on cursor (useful after mouse scroll)
      KeyCode::Char('c') if self.reading_mode => {
        self.center_cursor();
        self.mark_dirty();
        Ok(Some(false))
      }

      // Esc — stop playback and exit reading mode entirely
      KeyCode::Esc if self.reading_mode => {
        if let Some(vc) = &self.voice_controller {
          vc.stop();
          self.voice_started_at = None;
        }
        self.reading_mode = false;
        self.continuous_reading = false;
        self.mark_dirty();
        Ok(Some(false)) // consumed — don't run other Esc handlers
      }

      _ => Ok(None),
    }
  }

  // -----------------------------------------------------------------------
  // Continuous reading — advance to next paragraph and start playback.
  // Returns false when end of document is reached.
  // -----------------------------------------------------------------------

  pub fn advance_to_next_paragraph_for_continuous_reading(&mut self) -> bool {
    // Find first non-blank line after the last played paragraph
    let mut next = self.voice_para_end + 1;
    while next < self.lines.len() && self.lines[next].trim().is_empty() {
      next += 1;
    }
    if next >= self.lines.len() {
      return false;
    }

    // Move cursor to the new paragraph (centred in viewport)
    let half = self.height / 2;
    if next >= half {
      self.offset = next - half;
      self.cursor_y = half;
    } else {
      self.offset = 0;
      self.cursor_y = next;
    }

    let (text, start, end) = self.get_current_paragraph_with_lines();
    if text.is_empty() {
      return false;
    }
    self.voice_start(text, start, end);
    true
  }

  // -----------------------------------------------------------------------
  // Helpers
  // -----------------------------------------------------------------------

  pub fn voice_rendering_active(&self) -> bool {
    let voice_playing = matches!(self.voice_status, PlaybackStatus::Playing);
    if !voice_playing {
      return false;
    }

    let cursor_line = self.offset + self.cursor_y;
    let detached = self.reading_mode
      && (cursor_line < self.voice_para_start
        || cursor_line > self.voice_para_end);
    !detached
  }

  pub fn active_voice_word(&self) -> Option<(usize, usize, usize)> {
    if !self.voice_rendering_active() {
      return None;
    }

    let estimated_char_offset = if let Some(started) = self.voice_started_at {
      let elapsed_chars = (started.elapsed().as_secs_f32() * 13.0) as usize;
      self.voice_chars_before.saturating_add(elapsed_chars)
    } else {
      0
    };

    let paragraph_end =
      self.voice_para_end.min(self.lines.len().saturating_sub(1));
    let mut char_pos = 0usize;

    for document_line_index in self.voice_para_start..=paragraph_end {
      let line = &self.lines[document_line_index];
      let line_end = char_pos + line.len();
      if estimated_char_offset <= line_end {
        let column =
          estimated_char_offset.saturating_sub(char_pos).min(line.len());
        let (word_start, word_end) = Self::find_word_at(line, column);
        return Some((document_line_index, word_start, word_end));
      }
      char_pos = line_end + 1;
    }

    None
  }

  pub fn voice_status_label(&self) -> Option<String> {
    if let Some(err) = &self.voice_error {
      return Some(format!("[Voice: {err}]"));
    }

    match self.voice_status {
      PlaybackStatus::Loading => {
        use std::time::{SystemTime, UNIX_EPOCH};
        const FRAMES: &[char] =
          &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        let ms = SystemTime::now()
          .duration_since(UNIX_EPOCH)
          .unwrap_or_default()
          .as_millis();
        let frame = FRAMES[(ms / 100) as usize % FRAMES.len()];
        Some(format!("[{frame} Loading]"))
      }
      PlaybackStatus::Playing => Some("[♪ Playing]".to_string()),
      PlaybackStatus::Paused => Some("[⏸ Paused]".to_string()),
      PlaybackStatus::Idle => None,
    }
  }

  pub fn voice_line_dimmed(&self, document_line_index: usize) -> bool {
    self.voice_rendering_active()
      && (document_line_index < self.voice_para_start
        || document_line_index > self.voice_para_end)
  }

  fn find_word_at(s: &str, col: usize) -> (usize, usize) {
    let col = col.min(s.len());
    let col = (0..=col).rev().find(|&i| s.is_char_boundary(i)).unwrap_or(0);
    let is_word = |c: char| c.is_alphanumeric() || c == '\'' || c == '\u{2019}';

    let start = s[..col]
      .rfind(|c: char| !is_word(c))
      .map(|i| i + s[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1))
      .unwrap_or(0);
    let end =
      s[col..].find(|c: char| !is_word(c)).map(|i| col + i).unwrap_or(s.len());

    if start >= end {
      let next = ((col + 1)..=s.len())
        .find(|&i| s.is_char_boundary(i))
        .unwrap_or(s.len());
      (col, next)
    } else {
      (start, end)
    }
  }

  /// Sync voice_status, voice_error, and playing_info from the controller.
  pub fn sync_voice_status(&mut self) {
    if let Some(vc) = &self.voice_controller {
      let controller_status = vc.status();
      // Don't overwrite Loading with Idle — wait for the background thread
      // to confirm it has actually started (Playing) or errored out.
      let should_update = match (&self.voice_status, &controller_status) {
        (PlaybackStatus::Loading, PlaybackStatus::Idle) => false,
        _ => true,
      };
      if should_update {
        self.voice_status = controller_status;
      }
      if let Some(err) = vc.take_error() {
        self.voice_error = Some(err);
        self.voice_status = PlaybackStatus::Idle;
        self.voice_started_at = None;
      }

      // Read playing info for paragraph dimming + word highlight
      if let Ok(info_guard) = vc.playing_info.lock() {
        if let Some(info) = info_guard.as_ref() {
          self.voice_para_start = info.doc_start_line;
          self.voice_para_end = info.doc_end_line;
          self.voice_started_at = Some(info.started_at);
          self.voice_chars_before = info.chars_before_chunk;
        }
      }

      // Clear timing when idle/paused so effects disappear
      if matches!(
        self.voice_status,
        PlaybackStatus::Idle | PlaybackStatus::Paused
      ) {
        self.voice_started_at = None;
      }
    }
  }

  pub(super) fn voice_start(
    &mut self,
    text: String,
    doc_start_line: usize,
    doc_end_line: usize,
  ) {
    if let Some(vc) = &self.voice_controller {
      self.voice_status = PlaybackStatus::Loading;
      self.voice_error = None;
      self.voice_para_start = doc_start_line;
      self.voice_para_end = doc_end_line;
      self.voice_started_at = None;
      self.voice_chars_before = 0;
      vc.start(text, doc_start_line, doc_end_line);
    } else {
      self.voice_error = Some("No API key configured".to_string());
    }
  }

  // -----------------------------------------------------------------------
  // Text extraction
  // -----------------------------------------------------------------------

  /// Return the paragraph containing the cursor line, plus its doc line range.
  pub(super) fn get_current_paragraph_with_lines(
    &self,
  ) -> (String, usize, usize) {
    let current =
      (self.offset + self.cursor_y).min(self.lines.len().saturating_sub(1));

    if self.lines.is_empty() {
      return (String::new(), 0, 0);
    }

    // Walk backwards to find paragraph start
    let mut start = current;
    while start > 0 && !self.lines[start.saturating_sub(1)].trim().is_empty() {
      start -= 1;
    }
    if start < self.lines.len() && self.lines[start].trim().is_empty() {
      start += 1;
    }

    // Walk forwards to find paragraph end
    let mut end = current;
    while end + 1 < self.lines.len() && !self.lines[end + 1].trim().is_empty() {
      end += 1;
    }

    if start > end || start >= self.lines.len() {
      return (String::new(), 0, 0);
    }

    let end = end.min(self.lines.len().saturating_sub(1));
    let text = self.lines[start..=end].join("\n");
    (text, start, end)
  }

  /// Return lines from the cursor to end of the current paragraph.
  fn get_text_from_cursor_to_end_of_paragraph_with_lines(
    &self,
  ) -> (String, usize, usize) {
    let current =
      (self.offset + self.cursor_y).min(self.lines.len().saturating_sub(1));

    if self.lines.is_empty() {
      return (String::new(), 0, 0);
    }

    // Walk forwards to find paragraph end
    let mut end = current;
    while end + 1 < self.lines.len() && !self.lines[end + 1].trim().is_empty() {
      end += 1;
    }

    let end = end.min(self.lines.len().saturating_sub(1));
    if current > end {
      return (String::new(), 0, 0);
    }
    let text = self.lines[current..=end].join("\n");
    (text, current, end)
  }
}
