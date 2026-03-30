use std::{
  error::Error,
  io::{self, IsTerminal, Stdout},
};

use crossterm::{
  cursor::{Hide, Show},
  event::{self, Event as CEvent, KeyEventKind},
  execute,
  terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode,
  },
};
use ratatui::{Terminal, backend::CrosstermBackend, layout::Rect};

use super::{actions::EditorAction, core::Editor, widget};

pub fn run(editor: &mut Editor) -> Result<(), Box<dyn Error>> {
  if !io::stdout().is_terminal() {
    editor.tick();
    return Ok(());
  }

  let mut runtime = StandaloneRuntime::enter()?;
  editor.mark_dirty();

  loop {
    if matches!(editor.tick(), EditorAction::Quit) {
      break;
    }

    if editor.check_needs_redraw() {
      runtime
        .terminal
        .draw(|frame| widget::draw(frame, frame.area(), editor))?;
      editor.cursor_moved = false;
    }

    if event::poll(editor.poll_timeout())? {
      match event::read()? {
        CEvent::Key(key_event) if key_event.kind == KeyEventKind::Press => {
          if matches!(editor.handle_key(key_event), EditorAction::Quit) {
            break;
          }
        }
        CEvent::Resize(width, height) => {
          editor.update_layout(Rect::new(0, 0, width, height));
        }
        _ => {}
      }
    }

    editor.persist_viewport_state()?;
  }

  Ok(())
}

struct StandaloneRuntime {
  terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl StandaloneRuntime {
  fn enter() -> Result<Self, Box<dyn Error>> {
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, Hide)?;

    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(Self { terminal })
  }
}

impl Drop for StandaloneRuntime {
  fn drop(&mut self) {
    let _ = disable_raw_mode();
    let _ = execute!(self.terminal.backend_mut(), Show, LeaveAlternateScreen);
    let _ = self.terminal.show_cursor();
  }
}
