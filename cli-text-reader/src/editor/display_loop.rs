use std::time::{Duration, Instant};

use super::{actions::EditorAction, core::Editor};
use crate::progress::save_progress_with_viewport;
use crate::voice::PlaybackStatus;

impl Editor {
  pub fn apply_initial_layout(&mut self, skip_first_center: bool) {
    if !skip_first_center {
      self.center_cursor();
    }
    self.initial_setup_complete = true;
    self.mark_dirty();
  }

  pub fn tick(&mut self) -> EditorAction {
    let prev_status = self.voice_status.clone();
    let prev_error = self.voice_error.clone();
    self.sync_voice_status();
    if self.voice_status != prev_status || self.voice_error != prev_error {
      self.mark_dirty();
    }

    if self.continuous_reading
      && matches!(prev_status, PlaybackStatus::Playing)
      && matches!(self.voice_status, PlaybackStatus::Idle)
      && self.voice_error.is_none()
    {
      if !self.advance_to_next_paragraph_for_continuous_reading() {
        self.continuous_reading = false;
      }
      self.mark_dirty();
    }

    if matches!(
      self.voice_status,
      PlaybackStatus::Loading | PlaybackStatus::Playing
    ) {
      self.mark_dirty();
    }

    if let Some(until) = self.settings_saved_until {
      if Instant::now() >= until {
        self.settings_saved_until = None;
      }
      self.mark_dirty();
    }

    if self.tutorial_demo_mode {
      if let Some(until) = self.demo_hint_until
        && Instant::now() > until
      {
        if self.demo_hint_text.is_some() {
          self.demo_hint_text = None;
          self.mark_dirty();
        }
        self.demo_hint_until = None;
      }

      if let Some(key_event) = self.check_demo_progress() {
        return self.handle_key(key_event);
      }

      if self.should_exit_after_demo() {
        return EditorAction::Quit;
      }
    }

    if self.needs_redraw {
      EditorAction::NeedsRedraw
    } else {
      EditorAction::None
    }
  }

  pub fn poll_timeout(&self) -> Duration {
    if self.needs_redraw || self.tutorial_demo_mode {
      Duration::from_millis(16)
    } else if matches!(self.voice_status, PlaybackStatus::Playing) {
      Duration::from_millis(50)
    } else if matches!(self.voice_status, PlaybackStatus::Loading) {
      Duration::from_millis(100)
    } else {
      Duration::from_millis(250)
    }
  }

  pub fn persist_viewport_state(
    &mut self,
  ) -> Result<(), Box<dyn std::error::Error>> {
    let current_line = self.offset + self.cursor_y;
    if current_line != self.last_offset
      || self.offset != self.last_saved_viewport_offset
    {
      save_progress_with_viewport(
        self.document_hash,
        current_line,
        self.total_lines,
        Some(self.offset),
        Some(self.cursor_y),
      )?;
      self.last_offset = current_line;
      self.last_saved_viewport_offset = self.offset;
    }
    Ok(())
  }
}
