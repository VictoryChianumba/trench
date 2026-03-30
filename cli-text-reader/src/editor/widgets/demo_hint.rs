use ratatui::{
  layout::Rect,
  prelude::*,
  widgets::{Block, Borders, Clear, Paragraph},
};

use crate::editor::Editor;

pub fn draw_demo_hint(frame: &mut Frame, editor: &Editor, area: Rect) {
  let Some(text) = editor.demo_hint_text.as_ref() else {
    return;
  };
  if text.is_empty() || area.width < 8 || area.height < 5 {
    return;
  }

  let lines = text.lines().collect::<Vec<_>>();
  let max_width =
    lines.iter().map(|line| line.chars().count()).max().unwrap_or(0);
  let width = (max_width as u16 + 6).min(area.width.saturating_sub(2)).max(8);
  let height =
    (lines.len() as u16 + 2).min(area.height.saturating_sub(2)).max(3);
  let popup_area = Rect {
    x: area.x + area.width.saturating_sub(width) / 2,
    y: area.y + area.height.saturating_sub(height + 1),
    width,
    height,
  };

  let content = lines
    .into_iter()
    .map(|line| {
      Line::from(Span::styled(
        line.to_string(),
        Style::default().fg(Color::Yellow),
      ))
    })
    .collect::<Vec<_>>();

  let block = Block::default()
    .borders(Borders::ALL)
    .border_style(Style::default().fg(Color::Yellow))
    .style(Style::default().bg(Color::Rgb(20, 20, 20)));

  frame.render_widget(Clear, popup_area);
  frame
    .render_widget(Paragraph::new(content).block(block).centered(), popup_area);
}
