use ratatui::{layout::Rect, prelude::*};

use super::{
  core::Editor,
  highlight_spans::{self, StyleRange},
};
use crate::voice::PlaybackStatus;

pub struct RenderedLine {
  pub document_line_index: Option<usize>,
  pub spans: Vec<Span<'static>>,
  pub line_style: Style,
  pub is_current_line: bool,
  pub is_dimmed_line: bool,
  pub is_overscroll_blank: bool,
}

impl RenderedLine {
  pub fn into_line(self) -> Line<'static> {
    Line::from(self.spans).style(self.line_style)
  }
}

pub fn build_viewport_lines(editor: &Editor, area: Rect) -> Vec<RenderedLine> {
  let voice_playing = is_voice_rendering_active(editor);
  let voice_word = voice_playing.then(|| active_voice_word(editor)).flatten();

  (0..area.height as usize)
    .map(|screen_row| {
      let document_line_index = editor.offset + screen_row;
      let is_current_line = screen_row == editor.cursor_y;
      let is_overscroll_blank = document_line_index >= editor.lines.len();
      let is_dimmed_line = voice_playing
        && !is_overscroll_blank
        && (document_line_index < editor.voice_para_start
          || document_line_index > editor.voice_para_end);
      let line_style = editor.current_line_style_for_row(screen_row);
      let content = if is_overscroll_blank {
        String::new()
      } else {
        editor.lines[document_line_index].clone()
      };

      RenderedLine {
        document_line_index: (!is_overscroll_blank)
          .then_some(document_line_index),
        spans: highlight_spans::build_styled_spans(
          editor,
          (!is_overscroll_blank).then_some(document_line_index),
          &content,
          &" ".repeat(content_x_offset(editor, area) as usize),
          if is_dimmed_line {
            Style::default().fg(Color::DarkGray)
          } else {
            Style::default()
          },
          voice_style_ranges(document_line_index, &content, voice_word),
        ),
        line_style,
        is_current_line,
        is_dimmed_line,
        is_overscroll_blank,
      }
    })
    .collect()
}

pub fn content_x_offset(editor: &Editor, area: Rect) -> u16 {
  let width = area.width as usize;
  if width > editor.col {
    width.saturating_sub(editor.col) as u16 / 2
  } else {
    0
  }
}

/// Build lines for one pane in a horizontal split.
///
/// `buffer_idx` selects which `BufferState` to render.
/// Voice effects are applied only for the main document buffer (index 0 in
/// normal mode, or whichever buffer carries the document in tutorial mode).
pub fn build_pane_lines(
  editor: &Editor,
  buffer_idx: usize,
  area: Rect,
) -> Vec<RenderedLine> {
  let Some(buffer) = editor.buffers.get(buffer_idx) else {
    return vec![];
  };

  let lines = &buffer.lines;
  let offset = buffer.offset;
  // cursor_y is relative to this buffer's viewport
  let cursor_y = buffer.cursor_y;
  let is_active_pane = buffer_idx == editor.active_buffer;

  // Voice effects only on the primary document buffer (buf 0 in normal mode)
  let is_doc_buffer = buffer_idx == 0;
  let voice_playing = is_doc_buffer && is_voice_rendering_active(editor);
  let voice_word = voice_playing.then(|| active_voice_word(editor)).flatten();

  let padding = " ".repeat(content_x_offset(editor, area) as usize);

  (0..area.height as usize)
    .map(|screen_row| {
      let document_line_index = offset + screen_row;
      let is_current_line = is_active_pane && screen_row == cursor_y;
      let is_overscroll_blank = document_line_index >= lines.len();
      let is_dimmed_line = voice_playing
        && !is_overscroll_blank
        && (document_line_index < editor.voice_para_start
          || document_line_index > editor.voice_para_end);

      let line_style = if is_current_line && editor.show_highlighter {
        Style::default().bg(Color::Rgb(40, 40, 40))
      } else {
        Style::default()
      };

      let content = if is_overscroll_blank {
        String::new()
      } else {
        lines[document_line_index].clone()
      };

      // For the active pane, apply full highlight compositor.
      // For inactive pane, skip selection/search highlights.
      let doc_idx_opt = (!is_overscroll_blank).then_some(document_line_index);
      let highlight_doc_idx = if is_active_pane { doc_idx_opt } else { None };

      RenderedLine {
        document_line_index: doc_idx_opt,
        spans: highlight_spans::build_styled_spans(
          editor,
          highlight_doc_idx,
          &content,
          &padding,
          if is_dimmed_line {
            Style::default().fg(Color::DarkGray)
          } else {
            Style::default()
          },
          voice_style_ranges(document_line_index, &content, voice_word),
        ),
        line_style,
        is_current_line,
        is_dimmed_line,
        is_overscroll_blank,
      }
    })
    .collect()
}

fn voice_style_ranges(
  document_line_index: usize,
  content: &str,
  voice_word: Option<(usize, usize, usize)>,
) -> Vec<StyleRange> {
  if let Some((line_index, word_start, word_end)) = voice_word
    && line_index == document_line_index
    && word_start < word_end
    && !content.is_empty()
  {
    let word_start = word_start.min(content.len());
    let word_end = word_end.min(content.len());
    return vec![StyleRange {
      start: word_start,
      end: word_end,
      style: Style::default().add_modifier(Modifier::REVERSED),
      priority: 40,
    }];
  }

  Vec::new()
}

fn is_voice_rendering_active(editor: &Editor) -> bool {
  let voice_playing = matches!(editor.voice_status, PlaybackStatus::Playing);
  if !voice_playing {
    return false;
  }

  let cursor_line = editor.offset + editor.cursor_y;
  let detached = editor.reading_mode
    && (cursor_line < editor.voice_para_start
      || cursor_line > editor.voice_para_end);
  !detached
}

fn active_voice_word(editor: &Editor) -> Option<(usize, usize, usize)> {
  let estimated_char_offset = if let Some(started) = editor.voice_started_at {
    let elapsed_chars = (started.elapsed().as_secs_f32() * 13.0) as usize;
    editor.voice_chars_before.saturating_add(elapsed_chars)
  } else {
    0
  };

  let paragraph_end =
    editor.voice_para_end.min(editor.lines.len().saturating_sub(1));
  let mut char_pos = 0usize;

  for document_line_index in editor.voice_para_start..=paragraph_end {
    let line = &editor.lines[document_line_index];
    let line_end = char_pos + line.len();
    if estimated_char_offset <= line_end {
      let column =
        estimated_char_offset.saturating_sub(char_pos).min(line.len());
      let (word_start, word_end) = find_word_at(line, column);
      return Some((document_line_index, word_start, word_end));
    }
    char_pos = line_end + 1;
  }

  None
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
    let next =
      ((col + 1)..=s.len()).find(|&i| s.is_char_boundary(i)).unwrap_or(s.len());
    (col, next)
  } else {
    (start, end)
  }
}
