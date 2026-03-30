use crossterm::event::KeyEvent;
use ratatui::{Frame, layout::Rect};

use super::{App, HandleInputReturn};

/// Render the notes UI into `area` within the caller's frame.
pub fn draw(frame: &mut Frame, area: Rect, app: &mut App) {
    app.draw(frame, area);
}

/// Forward a key event to the notes app.
/// Returns `true` when notes wants to quit (caller should hide the pane).
pub fn handle_key(key: KeyEvent, app: &mut App) -> bool {
    matches!(app.handle_input(key), HandleInputReturn::ExitApp)
}
