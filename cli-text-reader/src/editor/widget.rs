use ratatui::{
  layout::{Constraint, Layout, Position},
  prelude::*,
  widgets::Paragraph,
};

use super::{
  core::{Editor, EditorMode, ViewMode},
  layout, render_lines,
  widgets::{demo_hint, settings_popup, status_bar},
};

pub fn draw(frame: &mut Frame, area: Rect, editor: &mut Editor) {
  if area.is_empty() {
    return;
  }

  editor.update_layout(area);

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

  let status = status_bar::build_status_line(editor);
  frame.render_widget(Paragraph::new(status), status_area);

  if editor.show_settings {
    settings_popup::draw_settings_popup(frame, editor, area);
  }
  if editor.tutorial_demo_mode {
    demo_hint::draw_demo_hint(frame, editor, area);
  }

  set_cursor(frame, editor, content_area, status_area);
}

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
        frame.set_cursor_position(Position::new(cursor_x, cursor_y));
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
        frame.set_cursor_position(Position::new(cursor_x, status_area.y));
      }
    }
    EditorMode::Tutorial => {}
  }
}
