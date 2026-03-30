use super::core::{Editor, PendingInput};
use crossterm::event::KeyCode;

impl Editor {
  // Handle text object keys in visual mode (i/a prefix commands)
  pub fn handle_visual_text_object_keys(
    &mut self,
    key_code: KeyCode,
  ) -> Result<Option<bool>, Box<dyn std::error::Error>> {
    match key_code {
      KeyCode::Char('i') => {
        self.begin_pending_input(PendingInput::VisualTextObject {
          around: false,
        })?;
        Ok(Some(false))
      }
      KeyCode::Char('a') => {
        self.begin_pending_input(PendingInput::VisualTextObject {
          around: true,
        })?;
        Ok(Some(false))
      }
      _ => Ok(None),
    }
  }
}
