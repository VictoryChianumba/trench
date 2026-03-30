use ratatui::{
  layout::{Constraint, Layout},
  prelude::*,
  widgets::Paragraph,
};

use super::core::{Editor, EditorMode};
use crate::voice::PlaybackStatus;

pub fn draw(frame: &mut Frame, area: Rect, editor: &mut Editor) {
  if area.is_empty() {
    return;
  }

  editor.update_layout(area);

  // Content area + 1-row status line at the bottom.
  let [content_area, status_area] =
    Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);

  // Render document content.
  let content = build_content_lines(editor, content_area);
  frame.render_widget(Paragraph::new(content), content_area);

  // Render status line.
  let status = build_status_line(editor);
  frame.render_widget(Paragraph::new(status), status_area);

  // Position the terminal cursor.
  set_cursor(frame, editor, content_area, status_area);
}

// ---------------------------------------------------------------------------
// Center-offset helpers
// ---------------------------------------------------------------------------

fn center_offset(editor: &Editor) -> usize {
  if editor.width > editor.col {
    (editor.width / 2).saturating_sub(editor.col / 2)
  } else {
    0
  }
}

// ---------------------------------------------------------------------------
// Voice-word helpers
// ---------------------------------------------------------------------------

/// Byte-range of the word under `col` in `s`. Returns `(start, end)` exclusive.
fn find_word_at(s: &str, col: usize) -> (usize, usize) {
  let col = col.min(s.len());
  let col = (0..=col).rev().find(|&i| s.is_char_boundary(i)).unwrap_or(0);

  let is_word = |c: char| c.is_alphanumeric() || c == '\'' || c == '\u{2019}';

  let start = s[..col]
    .rfind(|c: char| !is_word(c))
    .map(|i| i + s[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1))
    .unwrap_or(0);

  let end = s[col..]
    .find(|c: char| !is_word(c))
    .map(|i| col + i)
    .unwrap_or(s.len());

  if start >= end {
    let next =
      ((col + 1)..=s.len()).find(|&i| s.is_char_boundary(i)).unwrap_or(s.len());
    (col, next)
  } else {
    (start, end)
  }
}

/// Returns `(doc_line, byte_start, byte_end)` of the currently-playing word,
/// or `None` when voice is not active / cursor has detached from the paragraph.
fn voice_word(editor: &Editor) -> Option<(usize, usize, usize)> {
  let vp = matches!(editor.voice_status, PlaybackStatus::Playing);
  if !vp {
    return None;
  }
  let cursor_line = editor.offset + editor.cursor_y;
  let detached = editor.reading_mode
    && (cursor_line < editor.voice_para_start
      || cursor_line > editor.voice_para_end);
  if detached {
    return None;
  }

  let est_char = if let Some(started) = editor.voice_started_at {
    let elapsed = (started.elapsed().as_secs_f32() * 13.0) as usize;
    editor.voice_chars_before.saturating_add(elapsed)
  } else {
    0
  };

  let para_end = editor
    .voice_para_end
    .min(editor.lines.len().saturating_sub(1));
  let mut char_pos: usize = 0;
  for doc_line in editor.voice_para_start..=para_end {
    let line = &editor.lines[doc_line];
    let line_end = char_pos + line.len();
    if est_char <= line_end {
      let col = est_char.saturating_sub(char_pos).min(line.len());
      let (ws, we) = find_word_at(line, col);
      return Some((doc_line, ws, we));
    }
    char_pos = line_end + 1;
  }
  None
}

// ---------------------------------------------------------------------------
// Content rendering
// ---------------------------------------------------------------------------

fn build_content_lines(editor: &Editor, area: Rect) -> Vec<Line<'static>> {
  let height = area.height as usize;
  let padding = " ".repeat(center_offset(editor));

  let voice_playing = {
    let vp = matches!(editor.voice_status, PlaybackStatus::Playing);
    let cursor_line = editor.offset + editor.cursor_y;
    let detached = vp
      && editor.reading_mode
      && (cursor_line < editor.voice_para_start
        || cursor_line > editor.voice_para_end);
    vp && !detached
  };

  let word = if voice_playing { voice_word(editor) } else { None };

  (0..height)
    .map(|screen_row| {
      let doc_idx = editor.offset + screen_row;
      let is_cursor_row = screen_row == editor.cursor_y;
      let is_dimmed = voice_playing
        && (doc_idx < editor.voice_para_start
          || doc_idx > editor.voice_para_end);

      let bg = if is_cursor_row && editor.show_highlighter {
        Color::Rgb(40, 40, 40)
      } else {
        Color::Reset
      };
      let row_style = Style::default().bg(bg);

      let text: String = if doc_idx < editor.lines.len() {
        editor.lines[doc_idx].clone()
      } else {
        String::new()
      };

      // Dimmed voice line
      if is_dimmed {
        return Line::from(Span::styled(
          format!("{padding}{text}"),
          Style::default().fg(Color::DarkGray),
        ))
        .style(row_style);
      }

      // Voice-word highlight
      if let Some((wl, ws, we)) = word {
        if wl == doc_idx && ws < we && !text.is_empty() {
          let ws = ws.min(text.len());
          let we = we.min(text.len());
          return Line::from(vec![
            Span::raw(format!("{padding}{}", &text[..ws])),
            Span::styled(
              text[ws..we].to_string(),
              Style::default().add_modifier(Modifier::REVERSED),
            ),
            Span::raw(text[we..].to_string()),
          ])
          .style(row_style);
        }
      }

      // Plain line
      Line::from(format!("{padding}{text}")).style(row_style)
    })
    .collect()
}

// ---------------------------------------------------------------------------
// Status line
// ---------------------------------------------------------------------------

fn build_status_line(editor: &Editor) -> Line<'static> {
  let left = match editor.get_active_mode() {
    EditorMode::Command | EditorMode::CommandExecution => {
      format!(":{}", editor.get_active_command_buffer())
    }
    EditorMode::Search => format!("/{}", editor.get_active_command_buffer()),
    EditorMode::ReverseSearch => {
      format!("?{}", editor.get_active_command_buffer())
    }
    EditorMode::VisualChar => "-- VISUAL --".to_string(),
    EditorMode::VisualLine => "-- VISUAL LINE --".to_string(),
    EditorMode::Tutorial => "-- TUTORIAL --".to_string(),
    EditorMode::Normal => {
      if editor.reading_mode {
        if editor.continuous_reading {
          "-- READING >> --".to_string()
        } else {
          "-- READING --".to_string()
        }
      } else {
        String::new()
      }
    }
  };

  // Voice indicator (right side, shown in the last content row via the status
  // line row for now — Stage 7 will refine this)
  let voice_indicator: Option<String> = if let Some(err) = &editor.voice_error {
    Some(format!("[Voice: {err}]"))
  } else {
    match editor.voice_status {
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
  };

  // Progress (right-aligned suffix when enabled)
  let progress: Option<String> =
    if editor.show_progress && !editor.tutorial_demo_mode {
      let pos = (editor.offset + editor.cursor_y + 1).min(editor.total_lines);
      let pct = if editor.total_lines > 0 {
        (pos as f64 / editor.total_lines as f64 * 100.0)
          .round()
          .min(100.0) as u32
      } else {
        100
      };
      Some(format!("{pct}%"))
    } else {
      None
    };

  // Compose spans: left text + right-aligned items
  let mut spans: Vec<Span<'static>> = vec![Span::raw(left)];
  if let Some(vi) = voice_indicator {
    spans.push(Span::raw(format!("  {vi}")));
  }
  if let Some(p) = progress {
    spans.push(Span::raw(format!("  {p}")));
  }

  Line::from(spans)
}

// ---------------------------------------------------------------------------
// Cursor positioning
// ---------------------------------------------------------------------------

fn set_cursor(
  frame: &mut Frame,
  editor: &Editor,
  content_area: Rect,
  status_area: Rect,
) {
  if !editor.show_cursor || editor.show_settings {
    return;
  }

  let co = center_offset(editor) as u16;

  match editor.get_active_mode() {
    EditorMode::Normal | EditorMode::VisualChar | EditorMode::VisualLine => {
      let cx = content_area.x + co + editor.cursor_x as u16;
      let cy = content_area.y + editor.cursor_y as u16;
      if cy < content_area.bottom() && cx < content_area.right() {
        frame.set_cursor_position((cx, cy));
      }
    }
    EditorMode::Command
    | EditorMode::CommandExecution
    | EditorMode::Search
    | EditorMode::ReverseSearch => {
      let prefix_len: u16 = 1; // ":" / "/" / "?"
      let cmd_col = prefix_len + editor.get_active_command_cursor_pos() as u16;
      let cx = status_area.x + cmd_col;
      if cx < status_area.right() {
        frame.set_cursor_position((cx, status_area.y));
      }
    }
    EditorMode::Tutorial => {}
  }
}
