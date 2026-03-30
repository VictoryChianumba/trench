use ratatui::layout::{Constraint, Layout, Rect};

use super::core::Editor;

pub struct SplitLayout {
  pub top_area: Rect,
  pub separator_area: Rect,
  pub bottom_area: Rect,
  pub top_buffer_idx: usize,
  pub bottom_buffer_idx: usize,
}

pub fn split_buffer_indices(editor: &Editor) -> Option<(usize, usize)> {
  if editor.tutorial_active && editor.buffers.len() > 2 {
    Some((1, 2))
  } else if editor.buffers.len() > 1 {
    Some((0, 1))
  } else {
    None
  }
}

pub fn split_layout(editor: &Editor, area: Rect) -> Option<SplitLayout> {
  let (top_buffer_idx, bottom_buffer_idx) = split_buffer_indices(editor)?;
  if area.height < 3 {
    return None;
  }

  let content_height = area.height.saturating_sub(1);
  let top_height =
    ((content_height as f32) * editor.split_ratio).round() as u16;
  let top_height = top_height.clamp(1, content_height.saturating_sub(1));
  let bottom_height = content_height.saturating_sub(top_height);

  let [top_area, separator_area, bottom_area] = Layout::vertical([
    Constraint::Length(top_height),
    Constraint::Length(1),
    Constraint::Length(bottom_height),
  ])
  .areas(area);

  Some(SplitLayout {
    top_area,
    separator_area,
    bottom_area,
    top_buffer_idx,
    bottom_buffer_idx,
  })
}

pub fn sync_split_viewports(editor: &mut Editor, split_layout: &SplitLayout) {
  if let Some(buffer) = editor.buffers.get_mut(split_layout.top_buffer_idx) {
    buffer.viewport_height = split_layout.top_area.height as usize;
    buffer.split_height = Some(split_layout.top_area.height as usize);
  }
  if let Some(buffer) = editor.buffers.get_mut(split_layout.bottom_buffer_idx) {
    buffer.viewport_height = split_layout.bottom_area.height as usize;
    buffer.split_height = Some(split_layout.bottom_area.height as usize);
  }
}
