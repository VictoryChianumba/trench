use ratatui::{
  layout::{Constraint, Layout},
  prelude::*,
  widgets::Paragraph,
};

use super::{
  core::{Editor, EditorMode, ViewMode},
  layout, render_lines,
};
use crate::voice::PlaybackStatus;

pub fn draw(frame: &mut Frame, area: Rect, editor: &mut Editor) {
  if area.is_empty() {
    return;
  }

  editor.update_layout(area);

  // Layout: content rows (or split content) + 1-row status line.
  let [content_area, status_area] =
    Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);

  match editor.view_mode {
    ViewMode::HorizontalSplit => {
      draw_split(frame, editor, content_area);
    }
    ViewMode::Normal | ViewMode::Overlay => {
      let content = render_lines::build_viewport_lines(editor, content_area)
        .into_iter()
        .map(|line| line.into_line())
        .collect::<Vec<_>>();
      frame.render_widget(Paragraph::new(content), content_area);
    }
  }

  let status = build_status_line(editor);
  frame.render_widget(Paragraph::new(status), status_area);

  set_cursor(frame, editor, content_area, status_area);
}

/// Render a horizontal split: top pane ─── separator ─── bottom pane.
fn draw_split(frame: &mut Frame, editor: &mut Editor, area: Rect) {
  let Some(split_layout) = layout::split_layout(editor, area) else {
    return;
  };
  layout::sync_split_viewports(editor, &split_layout);

  let top_lines = render_lines::build_pane_lines(
    editor,
    split_layout.top_buffer_idx,
    split_layout.top_area,
  )
  .into_iter()
  .map(|line| line.into_line())
  .collect::<Vec<_>>();
  frame.render_widget(Paragraph::new(top_lines), split_layout.top_area);

  frame.render_widget(
    Paragraph::new(Line::from(Span::styled(
      "─".repeat(split_layout.separator_area.width as usize),
      Style::default().fg(Color::DarkGray),
    ))),
    split_layout.separator_area,
  );

  let bottom_lines = render_lines::build_pane_lines(
    editor,
    split_layout.bottom_buffer_idx,
    split_layout.bottom_area,
  )
  .into_iter()
  .map(|line| line.into_line())
  .collect::<Vec<_>>();
  frame.render_widget(Paragraph::new(bottom_lines), split_layout.bottom_area);
}

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

  let progress = if editor.show_progress && !editor.tutorial_demo_mode {
    let pos = (editor.offset + editor.cursor_y + 1).min(editor.total_lines);
    let pct = if editor.total_lines > 0 {
      (pos as f64 / editor.total_lines as f64 * 100.0).round().min(100.0) as u32
    } else {
      100
    };
    Some(format!("{pct}%"))
  } else {
    None
  };

  let mut spans: Vec<Span<'static>> = vec![Span::raw(left)];
  if let Some(voice_indicator) = voice_indicator {
    spans.push(Span::raw(format!("  {voice_indicator}")));
  }
  if let Some(progress) = progress {
    spans.push(Span::raw(format!("  {progress}")));
  }

  Line::from(spans)
}

fn set_cursor(
  frame: &mut Frame,
  editor: &Editor,
  content_area: Rect,
  status_area: Rect,
) {
  if !editor.show_cursor || editor.show_settings {
    return;
  }

  let normal_cursor_area = if editor.view_mode == ViewMode::HorizontalSplit {
    layout::split_layout(editor, content_area)
      .map(|split_layout| {
        if editor.active_pane == 0 {
          split_layout.top_area
        } else {
          split_layout.bottom_area
        }
      })
      .unwrap_or(content_area)
  } else {
    content_area
  };
  let center_offset =
    render_lines::content_x_offset(editor, normal_cursor_area);

  match editor.get_active_mode() {
    EditorMode::Normal | EditorMode::VisualChar | EditorMode::VisualLine => {
      let cursor_x =
        normal_cursor_area.x + center_offset + editor.cursor_x as u16;
      let cursor_y = normal_cursor_area.y + editor.cursor_y as u16;
      if cursor_y < normal_cursor_area.bottom()
        && cursor_x < normal_cursor_area.right()
      {
        frame.set_cursor_position((cursor_x, cursor_y));
      }
    }
    EditorMode::Command
    | EditorMode::CommandExecution
    | EditorMode::Search
    | EditorMode::ReverseSearch => {
      let prefix_len: u16 = 1;
      let cursor_x = status_area.x
        + prefix_len
        + editor.get_active_command_cursor_pos() as u16;
      if cursor_x < status_area.right() {
        frame.set_cursor_position((cursor_x, status_area.y));
      }
    }
    EditorMode::Tutorial => {}
  }
}
