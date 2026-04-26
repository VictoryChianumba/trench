use crossterm::{
  QueueableCommand,
  cursor::MoveTo,
  style::{Color, Print, ResetColor, SetForegroundColor},
};

use super::core::Editor;
use crate::config::{AppConfig, load_config, save_config};

const FIELD_NAMES: [&str; 3] =
  ["ELEVENLABS_API_KEY", "VOICE_ID", "PLAYBACK_SPEED"];
const POPUP_W: u16 = 60;
const POPUP_H: u16 = 14;

impl Editor {
  /// Open the settings popup, loading current values from config.
  pub fn open_settings_popup(&mut self) {
    let config = load_config();
    self.settings_fields[0] = config.elevenlabs_api_key.clone();
    self.settings_fields[1] = config.voice_id.clone();
    self.settings_fields[2] = format!("{:.1}", config.playback_speed);
    self.settings_cursor = 0;
    self.settings_editing = false;
    self.show_settings = true;
    self.mark_dirty();
  }

  /// Close the settings popup without saving.
  pub fn close_settings_popup(&mut self) {
    self.show_settings = false;
    self.settings_editing = false;
    self.mark_dirty();
  }

  /// Save settings to disk and reload voice config.
  pub fn save_settings_popup(&mut self) {
    let speed: f32 =
      self.settings_fields[2].parse::<f32>().unwrap_or(1.0).clamp(0.5, 2.0);
    // Normalise the displayed speed value after clamping
    self.settings_fields[2] = format!("{speed:.1}");

    let config = AppConfig {
      elevenlabs_api_key: self.settings_fields[0].clone(),
      voice_id: self.settings_fields[1].clone(),
      playback_speed: speed,
      ..Default::default()
    };
    let _ = save_config(&config);

    // Reload voice controller with new credentials
    let api_key = self.settings_fields[0].clone();
    let voice_id = self.settings_fields[1].clone();
    if api_key.is_empty() {
      self.voice_controller = None;
    } else {
      use crate::voice::playback::PlaybackController;
      self.voice_controller = Some(PlaybackController::new(api_key, voice_id));
    }

    self.settings_saved_until =
      Some(std::time::Instant::now() + std::time::Duration::from_secs(2));
    self.mark_dirty();
  }

  /// Handle a key event while the settings popup is open.
  /// Returns `Some(true)` to quit, `Some(false)` to consume, `None` to fall through.
  pub fn handle_settings_key(
    &mut self,
    key_event: crossterm::event::KeyEvent,
  ) -> Result<Option<bool>, Box<dyn std::error::Error>> {
    use crossterm::event::KeyCode;

    if self.settings_editing {
      // Text-entry mode for the active field
      match key_event.code {
        KeyCode::Esc | KeyCode::Enter => {
          // Validate PLAYBACK_SPEED on commit
          if self.settings_cursor == 2 {
            let v: f32 = self.settings_fields[2]
              .parse::<f32>()
              .unwrap_or(1.0)
              .clamp(0.5, 2.0);
            self.settings_fields[2] = format!("{v:.1}");
          }
          self.settings_editing = false;
          self.mark_dirty();
        }
        KeyCode::Char(c) => {
          self.settings_fields[self.settings_cursor].push(c);
          self.mark_dirty();
        }
        KeyCode::Backspace => {
          self.settings_fields[self.settings_cursor].pop();
          self.mark_dirty();
        }
        _ => {}
      }
      return Ok(Some(false));
    }

    // Navigation mode
    match key_event.code {
      KeyCode::Esc => {
        self.close_settings_popup();
      }
      KeyCode::Char('j') | KeyCode::Down => {
        self.settings_cursor = (self.settings_cursor + 1).min(2);
        self.mark_dirty();
      }
      KeyCode::Char('k') | KeyCode::Up => {
        self.settings_cursor = self.settings_cursor.saturating_sub(1);
        self.mark_dirty();
      }
      KeyCode::Enter => {
        self.settings_editing = true;
        self.mark_dirty();
      }
      KeyCode::Char('s') => {
        self.save_settings_popup();
      }
      _ => {}
    }
    Ok(Some(false))
  }

  /// Draw the settings popup into `buf` using crossterm queued commands.
  pub fn draw_settings_popup_buffered(
    &self,
    buf: &mut Vec<u8>,
  ) -> std::io::Result<()> {
    let term_w = self.width as u16;
    let term_h = self.height as u16;

    // Centre the popup
    let left = term_w.saturating_sub(POPUP_W) / 2;
    let top = term_h.saturating_sub(POPUP_H) / 2;

    // ── top border ──────────────────────────────────────────────────────────
    buf.queue(MoveTo(left, top))?;
    buf.queue(Print(format!(
      "┌─ Settings {:─<width$}┐",
      "",
      width = (POPUP_W as usize).saturating_sub(13)
    )))?;

    // ── inner rows ──────────────────────────────────────────────────────────
    let inner_w = (POPUP_W as usize).saturating_sub(2); // between │ borders

    for row in 1..POPUP_H - 1 {
      buf.queue(MoveTo(left, top + row))?;
      buf.queue(Print(format!("│{:<inner_w$}│", "")))?;
    }

    // ── bottom border ───────────────────────────────────────────────────────
    buf.queue(MoveTo(left, top + POPUP_H - 1))?;
    buf.queue(Print(format!(
      "└{:─<width$}┘",
      "",
      width = (POPUP_W as usize).saturating_sub(2)
    )))?;

    // ── field labels + values ───────────────────────────────────────────────
    // Layout:
    //   row 1: blank
    //   rows 2,3,4 → field 0  (label row, value row, blank row)
    //   rows 5,6,7 → field 1
    //   rows 8,9,10 → field 2
    //   row 11: hint
    //   row 12: hint2 / saved msg

    for (i, name) in FIELD_NAMES.iter().enumerate() {
      let label_row = top + 1 + (i as u16) * 3 + 1; // rows 2, 5, 8
      let value_row = label_row + 1;

      // Label
      let selected = i == self.settings_cursor;
      buf.queue(MoveTo(left + 2, label_row))?;
      if selected {
        buf.queue(SetForegroundColor(Color::Yellow))?;
        buf.queue(Print(format!("▸ {name}")))?;
        buf.queue(ResetColor)?;
      } else {
        buf.queue(Print(format!("  {name}")))?;
      }

      // Value (mask API key)
      let raw = &self.settings_fields[i];
      let display: String = if i == 0 && !raw.is_empty() {
        "*".repeat(raw.len())
      } else {
        raw.clone()
      };

      // Truncate to fit inside popup
      let max_val = (inner_w).saturating_sub(4);
      let display = if display.len() > max_val {
        format!("{}…", &display[..max_val.saturating_sub(1)])
      } else {
        display
      };

      buf.queue(MoveTo(left + 4, value_row))?;
      if selected && self.settings_editing {
        buf.queue(SetForegroundColor(Color::Cyan))?;
        buf.queue(Print(format!("{display}_")))?;
        buf.queue(ResetColor)?;
      } else {
        buf.queue(Print(display))?;
      }
    }

    // ── hint row ────────────────────────────────────────────────────────────
    let hint_row = top + POPUP_H - 3;
    buf.queue(MoveTo(left + 2, hint_row))?;
    buf.queue(SetForegroundColor(Color::DarkGrey))?;
    if self.settings_editing {
      buf.queue(Print("Type to edit  Enter/Esc: confirm"))?;
    } else {
      buf.queue(Print("j/k: move  Enter: edit  s: save  Esc: close"))?;
    }
    buf.queue(ResetColor)?;

    // ── saved confirmation ───────────────────────────────────────────────────
    let saved_row = top + POPUP_H - 2;
    if self
      .settings_saved_until
      .map_or(false, |t| std::time::Instant::now() < t)
    {
      buf.queue(MoveTo(left + 2, saved_row))?;
      buf.queue(SetForegroundColor(Color::Green))?;
      buf.queue(Print("Saved."))?;
      buf.queue(ResetColor)?;
    }

    Ok(())
  }
}
