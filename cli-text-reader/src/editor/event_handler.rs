use crossterm::event;

use super::{
  actions::EditorAction,
  core::{Editor, EditorMode},
};

impl Editor {
  pub fn handle_key(&mut self, key_event: event::KeyEvent) -> EditorAction {
    match self.handle_key_inner(key_event) {
      Ok(action) => action,
      Err(error) => {
        self.debug_log_error(&format!("handle_key failed: {error}"));
        EditorAction::NeedsRedraw
      }
    }
  }

  fn handle_key_inner(
    &mut self,
    key_event: event::KeyEvent,
  ) -> Result<EditorAction, Box<dyn std::error::Error>> {
    let active_mode = self.get_active_mode();
    self.debug_log(&format!(
      "=== handle_event: key={:?}, active_buffer={}, active_mode={:?}, view_mode={:?} ===",
      key_event, self.active_buffer, active_mode, self.view_mode
    ));
    // Settings popup intercepts all keys while open
    if self.show_settings {
      if let Some(result) = self.handle_settings_key(key_event)? {
        return Ok(if result {
          EditorAction::Quit
        } else {
          self.action_from_redraw_state()
        });
      }
    }

    // Route to mode-specific handlers first
    let result = match active_mode {
      EditorMode::Normal => self.handle_normal_mode_event(key_event),
      EditorMode::VisualChar | EditorMode::VisualLine => {
        self.handle_visual_mode_event(key_event)
      }
      EditorMode::Search | EditorMode::ReverseSearch => {
        self.handle_search_mode_event(key_event)
      }
      EditorMode::Command | EditorMode::CommandExecution => {
        self.handle_command_mode_event(key_event)
      }
      EditorMode::Tutorial => Ok(self.process_tutorial_key(key_event.code)),
    };

    // Process tutorial key after mode-specific handling, only if in tutorial
    // mode
    if self.tutorial_active {
      // Don't process tutorial keys if we're in a special input mode
      match self.get_active_mode() {
        EditorMode::Command
        | EditorMode::CommandExecution
        | EditorMode::Search
        | EditorMode::ReverseSearch => {
          // Skip tutorial processing for input modes
        }
        _ => {
          // Process tutorial key for other modes
          self.process_tutorial_key(key_event.code);
        }
      }
    }

    if result? {
      Ok(EditorAction::Quit)
    } else {
      Ok(self.action_from_redraw_state())
    }
  }

  fn action_from_redraw_state(&self) -> EditorAction {
    if self.needs_redraw {
      EditorAction::NeedsRedraw
    } else {
      EditorAction::None
    }
  }
}
