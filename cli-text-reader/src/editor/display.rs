use crossterm::{
  QueueableCommand, execute,
  style::{Color, ResetColor, SetBackgroundColor, SetForegroundColor},
  terminal::{Clear, ClearType},
};
use std::io::{self, Result as IoResult, Write};

use super::core::{Editor, ViewMode};

impl Editor {
  // Draw content with proper highlighting
  pub(super) fn draw_content(
    &self,
    stdout: &mut io::Stdout,
    term_width: u16,
    center_offset_string: &str,
  ) -> IoResult<()> {
    let content_height = self.height.saturating_sub(1);

    for i in 0..content_height {
      execute!(stdout, crossterm::cursor::MoveTo(0, i as u16))?;

      // Calculate the actual line index in the document
      let line_idx = self.offset + i;

      if line_idx < self.lines.len() {
        // We have a real line to display
        let line = self.lines[line_idx].clone();

        // Highlight the current line first
        let is_current_line =
          self.highlight_current_line(stdout, i, term_width)?;

        // Check if we need to render any special highlights
        let has_selection = self.has_selection_on_line(i);
        let has_search = self.has_search_match_on_line(i);
        let has_persistent = self.has_persistent_highlights_on_line(i);

        // If we have multiple types of highlights, use combined rendering
        if (has_search || has_selection) && has_persistent {
          // Render line with combined highlights
          if self.render_combined_highlights(
            stdout,
            i,
            &line,
            center_offset_string,
          )? {
            continue;
          }
        }

        // Try highlighting selection only
        if has_selection
          && self.highlight_selection(stdout, i, &line, center_offset_string)?
        {
          continue;
        }

        // Try highlighting search match only
        if has_search
          && self.highlight_search_match(
            stdout,
            i,
            &line,
            center_offset_string,
          )?
        {
          continue;
        }

        // Try highlighting persistent highlights only
        if has_persistent
          && self.highlight_persistent(
            stdout,
            i,
            &line,
            center_offset_string,
          )?
        {
          continue;
        }

        // Normal line rendering - if current line was highlighted,
        // we need to use appropriate text color
        if is_current_line {
          // For the highlighted line, use a color that contrasts with the
          // background
          execute!(
            stdout,
            crossterm::style::SetForegroundColor(
              crossterm::style::Color::White
            )
          )?;
          write!(stdout, "{center_offset_string}{line}")?;
          execute!(stdout, crossterm::style::ResetColor)?;
          // Don't clear the line since we want to keep the background color
        } else {
          write!(stdout, "{center_offset_string}{line}")?;
          // Clear to end of line to avoid artifacts
          execute!(
            stdout,
            crossterm::terminal::Clear(
              crossterm::terminal::ClearType::UntilNewLine
            )
          )?;
        }
      } else {
        // This is beyond the document - show blank line for overscroll
        // But still check if we need to highlight the cursor line
        let is_current_line =
          self.highlight_current_line(stdout, i, term_width)?;

        if is_current_line {
          // Show highlighted empty line for cursor position
          execute!(
            stdout,
            crossterm::style::SetForegroundColor(
              crossterm::style::Color::White
            )
          )?;
          write!(stdout, "{center_offset_string}")?;
          execute!(stdout, crossterm::style::ResetColor)?;
          // Don't clear the line since we want to keep the background color
        } else {
          // Just show blank line
          write!(stdout, "{center_offset_string}")?;
          // Clear to end of line
          execute!(
            stdout,
            crossterm::terminal::Clear(
              crossterm::terminal::ClearType::UntilNewLine
            )
          )?;
        }
      }
    }

    // Reset highlighting at the end of each frame
    if self.show_highlighter {
      execute!(stdout, SetBackgroundColor(crossterm::style::Color::Reset))?;
    }

    Ok(())
  }

  // Buffered version of draw_content
  pub(super) fn draw_content_buffered(
    &self,
    buffer: &mut Vec<u8>,
    term_width: u16,
    center_offset_string: &str,
  ) -> IoResult<()> {
    use crate::voice::PlaybackStatus;

    let content_height = self.height.saturating_sub(1);

    // --- Voice-mode rendering state ---
    // Only apply dimming/highlight effects while actively playing and the
    // cursor has not been moved away from the playing paragraph.
    let voice_playing = matches!(self.voice_status, PlaybackStatus::Playing);
    let cursor_doc_line = self.offset + self.cursor_y;
    let voice_cursor_detached = voice_playing
      && self.reading_mode
      && (cursor_doc_line < self.voice_para_start
        || cursor_doc_line > self.voice_para_end);
    let voice_playing = voice_playing && !voice_cursor_detached;

    // Estimate the current character offset within the paragraph text.
    // Used to pick which word to highlight.
    let est_char_offset: usize = if voice_playing {
      if let Some(started) = self.voice_started_at {
        let elapsed_chars = (started.elapsed().as_secs_f32() * 13.0) as usize;
        self.voice_chars_before.saturating_add(elapsed_chars)
      } else {
        0
      }
    } else {
      0
    };

    // Walk paragraph lines to locate which doc-line and col-range the
    // current word occupies.
    let voice_word: Option<(usize, usize, usize)> = if voice_playing {
      let para_start = self.voice_para_start;
      let para_end =
        self.voice_para_end.min(self.lines.len().saturating_sub(1));
      let mut char_pos: usize = 0;
      let mut found: Option<(usize, usize, usize)> = None;
      'outer: for doc_line in para_start..=para_end {
        let line = &self.lines[doc_line];
        let line_end = char_pos + line.len();
        if est_char_offset <= line_end {
          let col = est_char_offset.saturating_sub(char_pos).min(line.len());
          let (ws, we) = find_word_at(line, col);
          found = Some((doc_line, ws, we));
          break 'outer;
        }
        char_pos = line_end + 1; // +1 for newline between lines
      }
      found
    } else {
      None
    };

    for i in 0..content_height {
      buffer.queue(crossterm::cursor::MoveTo(0, i as u16))?;

      // Calculate the actual line index in the document
      let line_idx = self.offset + i;

      // Determine if this line is outside the active paragraph (dim it)
      let is_dimmed = voice_playing
        && (line_idx < self.voice_para_start || line_idx > self.voice_para_end);

      if line_idx < self.lines.len() {
        // We have a real line to display
        let line = self.lines[line_idx].clone();

        if is_dimmed {
          // Dim lines outside the active paragraph
          buffer.queue(crossterm::style::SetForegroundColor(
            crossterm::style::Color::DarkGrey,
          ))?;
          write!(buffer, "{center_offset_string}{line}")?;
          buffer.queue(crossterm::style::ResetColor)?;
          buffer.queue(crossterm::terminal::Clear(
            crossterm::terminal::ClearType::UntilNewLine,
          ))?;
          continue;
        }

        // Check if the current word highlight falls on this line
        if let Some((wl, ws, we)) = voice_word {
          if wl == line_idx && ws < we {
            // Render: prefix | highlighted word | suffix
            let prefix = &line[..ws.min(line.len())];
            let word = &line[ws.min(line.len())..we.min(line.len())];
            let suffix = &line[we.min(line.len())..];

            write!(buffer, "{center_offset_string}{prefix}")?;
            buffer.queue(crossterm::style::SetAttribute(
              crossterm::style::Attribute::Reverse,
            ))?;
            write!(buffer, "{word}")?;
            buffer.queue(crossterm::style::SetAttribute(
              crossterm::style::Attribute::Reset,
            ))?;
            write!(buffer, "{suffix}")?;
            buffer.queue(crossterm::terminal::Clear(
              crossterm::terminal::ClearType::UntilNewLine,
            ))?;
            continue;
          }
        }

        // Highlight the current line first
        let is_current_line =
          self.highlight_current_line_buffered(buffer, i, term_width)?;

        // Check if we need to render any special highlights
        let has_selection = self.has_selection_on_line(i);
        let has_search = self.has_search_match_on_line(i);
        let has_persistent = self.has_persistent_highlights_on_line(i);

        // If we have multiple types of highlights, use combined rendering
        if (has_search || has_selection) && has_persistent {
          // Render line with combined highlights
          if self.render_combined_highlights_buffered(
            buffer,
            i,
            &line,
            center_offset_string,
          )? {
            continue;
          }
        }

        // Try highlighting selection only
        if has_selection
          && self.highlight_selection_buffered(
            buffer,
            i,
            &line,
            center_offset_string,
          )?
        {
          continue;
        }

        // Try highlighting search match only
        if has_search
          && self.highlight_search_match_buffered(
            buffer,
            i,
            &line,
            center_offset_string,
          )?
        {
          continue;
        }

        // Try highlighting persistent highlights only
        if has_persistent
          && self.highlight_persistent_buffered(
            buffer,
            i,
            &line,
            center_offset_string,
          )?
        {
          continue;
        }

        // Normal line rendering
        if is_current_line {
          buffer.queue(crossterm::style::SetForegroundColor(
            crossterm::style::Color::White,
          ))?;
          write!(buffer, "{center_offset_string}{line}")?;
          buffer.queue(crossterm::style::ResetColor)?;
        } else {
          write!(buffer, "{center_offset_string}{line}")?;
          buffer.queue(crossterm::terminal::Clear(
            crossterm::terminal::ClearType::UntilNewLine,
          ))?;
        }
      } else {
        // This is beyond the document - show blank line for overscroll
        if is_dimmed {
          write!(buffer, "{center_offset_string}")?;
          buffer.queue(crossterm::terminal::Clear(
            crossterm::terminal::ClearType::UntilNewLine,
          ))?;
          continue;
        }

        let is_current_line =
          self.highlight_current_line_buffered(buffer, i, term_width)?;

        if is_current_line {
          buffer.queue(crossterm::style::SetForegroundColor(
            crossterm::style::Color::White,
          ))?;
          write!(buffer, "{center_offset_string}")?;
          buffer.queue(crossterm::style::ResetColor)?;
        } else {
          write!(buffer, "{center_offset_string}")?;
          buffer.queue(crossterm::terminal::Clear(
            crossterm::terminal::ClearType::UntilNewLine,
          ))?;
        }
      }
    }

    // Reset highlighting at the end of each frame
    if self.show_highlighter {
      buffer.queue(SetBackgroundColor(crossterm::style::Color::Reset))?;
    }

    Ok(())
  }
}

/// Find the byte-index boundaries of the word that overlaps `col` in `s`.
/// Returns `(start, end)` as byte indices (end is exclusive).
fn find_word_at(s: &str, col: usize) -> (usize, usize) {
  let col = col.min(s.len());
  // Snap to nearest valid UTF-8 char boundary at or before col
  let col = (0..=col).rev().find(|&i| s.is_char_boundary(i)).unwrap_or(0);

  // Scan left to find word start
  let is_word_char =
    |c: char| c.is_alphanumeric() || c == '\'' || c == '\u{2019}';

  let start = s[..col]
    .rfind(|c: char| !is_word_char(c))
    .map(|i| i + s[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1))
    .unwrap_or(0);

  // Scan right to find word end
  let end = s[col..]
    .find(|c: char| !is_word_char(c))
    .map(|i| col + i)
    .unwrap_or(s.len());

  if start >= end {
    // Advance to the next char boundary
    let next =
      ((col + 1)..=s.len()).find(|&i| s.is_char_boundary(i)).unwrap_or(s.len());
    (col, next)
  } else {
    (start, end)
  }
}
