use ratatui::prelude::*;

use crate::editor::{Editor, EditorMode};

pub fn build_status_line(editor: &Editor) -> Line<'static> {
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

  let voice_indicator = editor.voice_status_label();

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
