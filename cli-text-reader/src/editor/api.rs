use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, prelude::*};
use std::time::Duration;

use super::{actions::EditorAction, core::Editor};
use crate::core_types::EditorMode;
use crate::voice::PlaybackStatus;

impl Editor {
  // -----------------------------------------------------------------------
  // Runtime API — called by runtime.rs (ratatui path)
  // -----------------------------------------------------------------------

  /// Process one key event and return the resulting action.
  pub fn handle_key(&mut self, key_event: KeyEvent) -> EditorAction {
    let mut stdout = std::io::stdout();
    match self.handle_event(key_event, &mut stdout) {
      Ok(true) => EditorAction::Quit,
      Ok(false) => {
        self.mark_dirty();
        EditorAction::NeedsRedraw
      }
      Err(_) => EditorAction::None,
    }
  }

  /// Per-frame state update: voice sync, demo hints, continuous reading.
  pub fn tick(&mut self) -> EditorAction {
    // Demo hint expiry
    if let Some(until) = self.demo_hint_until
      && std::time::Instant::now() > until
    {
      if self.demo_hint_text.is_some() {
        self.demo_hint_text = None;
        self.demo_hint_until = None;
        self.mark_dirty();
      } else {
        self.demo_hint_until = None;
      }
    }

    // Sync voice playback status each tick
    self.sync_voice_status();

    // Advance continuous reading when current chunk finishes
    if self.continuous_reading
      && matches!(self.voice_status, PlaybackStatus::Idle)
    {
      if !self.advance_to_next_paragraph_for_continuous_reading() {
        self.continuous_reading = false;
      }
      self.mark_dirty();
    }

    EditorAction::None
  }

  /// Update internal width/height from a ratatui Rect (called on resize and
  /// before each draw).
  pub fn update_layout(&mut self, area: Rect) {
    let new_width = area.width as usize;
    let new_height = area.height as usize;
    if new_width != self.width || new_height != self.height {
      self.width = new_width;
      self.height = new_height;
      if self.initial_setup_complete {
        self.center_cursor();
      }
      self.mark_dirty();
    }
  }

  /// How long to wait for the next event before re-rendering.
  pub fn poll_timeout(&self) -> Duration {
    if self.needs_redraw || self.tutorial_demo_mode {
      Duration::from_millis(16) // ~60 fps when animating
    } else {
      Duration::from_millis(250) // idle
    }
  }

  /// Persist current viewport position to the progress file.
  pub fn persist_viewport_state(
    &mut self,
  ) -> Result<(), Box<dyn std::error::Error>> {
    let current_line = self.offset + self.cursor_y;
    if current_line != self.last_offset
      || self.offset != self.last_saved_viewport_offset
    {
      crate::progress::save_progress_with_viewport(
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

  // -----------------------------------------------------------------------
  // Query methods — called by highlight_spans.rs and render_lines.rs
  // -----------------------------------------------------------------------

  /// Return the byte range `(start, end)` of the current search match on
  /// `doc_line_idx`, or `None` if there is no match on that line.
  pub fn search_match_range_for_line(
    &self,
    doc_line_idx: usize,
    content: &str,
  ) -> Option<(usize, usize)> {
    let match_to_check = if self.editor_state.search_preview_active {
      self.editor_state.search_preview_match
    } else {
      self.editor_state.current_match
    };
    let (line_idx, start, end) = match_to_check?;
    if line_idx != doc_line_idx {
      return None;
    }
    let start = start.min(content.len());
    let end = end.min(content.len());
    if start < end { Some((start, end)) } else { None }
  }

  /// Return all persistent-highlight byte ranges on `doc_line_idx`.
  pub fn persistent_highlight_ranges_for_line(
    &self,
    doc_line_idx: usize,
    content: &str,
  ) -> Vec<(usize, usize)> {
    let mut abs_line_start = 0usize;
    for i in 0..doc_line_idx {
      if i < self.lines.len() {
        abs_line_start += self.lines[i].len() + 1;
      }
    }
    let abs_line_end = abs_line_start + content.len();

    let line_highlights =
      self.highlights.get_highlights_for_range(abs_line_start, abs_line_end);
    if line_highlights.is_empty() {
      return vec![];
    }

    let mut ranges: Vec<(usize, usize)> = line_highlights
      .iter()
      .filter_map(|h| {
        let start = if h.start <= abs_line_start {
          0
        } else {
          h.start - abs_line_start
        };
        let end = if h.end >= abs_line_end {
          content.len()
        } else {
          h.end - abs_line_start
        };
        if end > start && start < content.len() {
          Some((start.min(content.len()), end.min(content.len())))
        } else {
          None
        }
      })
      .collect();

    ranges.sort_by_key(|r| r.0);
    ranges
  }

  /// Return the visual-selection byte ranges on `doc_line_idx`.
  pub fn selection_ranges_for_line(
    &self,
    doc_line_idx: usize,
    content: &str,
  ) -> Vec<(usize, usize)> {
    let Some(sel_start) = self.editor_state.selection_start else {
      return vec![];
    };
    let Some(sel_end) = self.editor_state.selection_end else {
      return vec![];
    };

    let active = self.editor_state.mode == EditorMode::VisualChar
      || self.editor_state.mode == EditorMode::VisualLine
      || self.editor_state.visual_selection_active;
    if !active {
      return vec![];
    }

    let is_line_mode = self.editor_state.mode == EditorMode::VisualLine
      || (self.editor_state.visual_selection_active
        && self.editor_state.previous_visual_mode
          == Some(EditorMode::VisualLine));

    let min_line = sel_start.0.min(sel_end.0);
    let max_line = sel_start.0.max(sel_end.0);

    if doc_line_idx < min_line || doc_line_idx > max_line {
      return vec![];
    }

    if is_line_mode {
      return if content.is_empty() { vec![] } else { vec![(0, content.len())] };
    }

    // Character mode
    let range = if sel_start.0 == sel_end.0 {
      let sc = sel_start.1.min(sel_end.1).min(content.len());
      let ec = sel_start.1.max(sel_end.1).min(content.len());
      (sc, ec)
    } else if doc_line_idx == min_line {
      let col =
        if sel_start.0 < sel_end.0 { sel_start.1 } else { sel_end.1 };
      (col.min(content.len()), content.len())
    } else if doc_line_idx == max_line {
      let col =
        if sel_start.0 > sel_end.0 { sel_start.1 } else { sel_end.1 };
      (0, col.min(content.len()))
    } else {
      (0, content.len())
    };

    if range.0 < range.1 { vec![range] } else { vec![] }
  }

  /// Background colour to apply to an entire screen row.
  pub fn current_line_style_for_row(&self, screen_row: usize) -> Style {
    if screen_row == self.cursor_y && self.show_highlighter {
      Style::default().bg(Color::Rgb(40, 40, 40))
    } else {
      Style::default()
    }
  }
}
