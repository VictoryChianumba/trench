use ratatui::{
  layout::{Constraint, Layout, Position},
  prelude::*,
  widgets::{Block, Clear, Paragraph},
};

const POPUP_W: u16 = 60;
const POPUP_H: u16 = 14;
const FIELD_NAMES: [&str; 3] = ["ELEVENLABS_API_KEY", "VOICE_ID", "PLAYBACK_SPEED"];

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

  if editor.show_settings {
    draw_settings_popup(frame, editor, area);
  }

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

fn draw_settings_popup(frame: &mut Frame, editor: &Editor, area: Rect) {
  let left = area.x + area.width.saturating_sub(POPUP_W) / 2;
  let top = area.y + area.height.saturating_sub(POPUP_H) / 2;
  let popup_area = Rect { x: left, y: top, width: POPUP_W, height: POPUP_H };

  frame.render_widget(Clear, popup_area);
  frame.render_widget(
    Block::bordered().title(" Settings "),
    popup_area,
  );

  let inner_w = (POPUP_W as usize).saturating_sub(2);
  let max_val = inner_w.saturating_sub(4);

  for (i, name) in FIELD_NAMES.iter().enumerate() {
    let label_row = top + 1 + (i as u16) * 3 + 1;
    let value_row = label_row + 1;
    let selected = i == editor.settings_cursor;

    let label_style = if selected {
      Style::default().fg(Color::Yellow)
    } else {
      Style::default()
    };
    let label = if selected {
      format!("▸ {name}")
    } else {
      format!("  {name}")
    };
    frame.render_widget(
      Paragraph::new(label).style(label_style),
      Rect { x: left + 2, y: label_row, width: POPUP_W - 4, height: 1 },
    );

    let raw = &editor.settings_fields[i];
    let display: String = if i == 0 && !raw.is_empty() {
      "*".repeat(raw.len())
    } else {
      raw.clone()
    };
    let display = if display.len() > max_val {
      format!("{}…", &display[..max_val.saturating_sub(1)])
    } else {
      display
    };
    let (value_text, value_style) = if selected && editor.settings_editing {
      (format!("{display}_"), Style::default().fg(Color::Cyan))
    } else {
      (display, Style::default())
    };
    frame.render_widget(
      Paragraph::new(value_text).style(value_style),
      Rect { x: left + 4, y: value_row, width: POPUP_W - 8, height: 1 },
    );
  }

  let hint_row = top + POPUP_H - 3;
  let hint = if editor.settings_editing {
    "Type to edit  Enter/Esc: confirm"
  } else {
    "j/k: move  Enter: edit  s: save  Esc: close"
  };
  frame.render_widget(
    Paragraph::new(hint).style(Style::default().fg(Color::DarkGray)),
    Rect { x: left + 2, y: hint_row, width: POPUP_W - 4, height: 1 },
  );

  let saved_row = top + POPUP_H - 2;
  if editor
    .settings_saved_until
    .map_or(false, |t| std::time::Instant::now() < t)
  {
    frame.render_widget(
      Paragraph::new("Saved.").style(Style::default().fg(Color::Green)),
      Rect { x: left + 2, y: saved_row, width: POPUP_W - 4, height: 1 },
    );
  }
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
