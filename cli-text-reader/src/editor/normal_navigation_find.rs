use super::core::{Editor, PendingInput};
use crossterm::event::KeyCode;

impl Editor {
  // Handle character finding keys (f/F/t/T)
  pub fn handle_char_find_keys(
    &mut self,
    key_code: KeyCode,
  ) -> Result<Option<bool>, Box<dyn std::error::Error>> {
    match key_code {
      KeyCode::Char('f') => {
        self.begin_pending_input(PendingInput::CharFind {
          forward: true,
          till: false,
          visual: false,
        })?;
        Ok(Some(false))
      }
      KeyCode::Char('F') => {
        self.begin_pending_input(PendingInput::CharFind {
          forward: false,
          till: false,
          visual: false,
        })?;
        Ok(Some(false))
      }
      KeyCode::Char('t') => {
        self.begin_pending_input(PendingInput::CharFind {
          forward: true,
          till: true,
          visual: false,
        })?;
        Ok(Some(false))
      }
      KeyCode::Char('T') => {
        self.begin_pending_input(PendingInput::CharFind {
          forward: false,
          till: true,
          visual: false,
        })?;
        Ok(Some(false))
      }
      _ => Ok(None),
    }
  }
}
