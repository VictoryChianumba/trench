use ratatui::{
  prelude::*,
  widgets::{Block, Paragraph, Wrap},
};

use super::core::Editor;

pub fn draw(frame: &mut Frame, area: Rect, editor: &mut Editor) {
  if area.is_empty() {
    return;
  }

  editor.update_layout(area);

  let content = vec![
    Line::from("ratatui scaffold active"),
    Line::from(format!("mode: {:?}", editor.get_active_mode())),
    Line::from(format!(
      "cursor: ({}, {})  offset: {}",
      editor.cursor_x, editor.cursor_y, editor.offset
    )),
    Line::from(format!(
      "viewport: {}x{}  buffers: {}",
      editor.width,
      editor.height,
      editor.buffers.len()
    )),
  ];

  let widget = Paragraph::new(content)
    .block(Block::bordered().title("cli-text-reader"))
    .wrap(Wrap { trim: false });
  frame.render_widget(widget, area);
}
