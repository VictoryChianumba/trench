mod nav;
mod progress;
mod render;
mod state;

use crossterm::{
  event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
  execute,
  terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use doc_model::Block;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use state::{Mode, Reader};
use std::io;

pub use state::PaperMeta;

pub fn run(
  blocks: Vec<Block>,
  meta: Option<PaperMeta>,
  progress_key: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
  enable_raw_mode()?;
  let mut stdout = io::stdout();
  execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
  let backend = CrosstermBackend::new(stdout);
  let mut terminal = Terminal::new(backend)?;

  let size = terminal.size()?;
  let mut reader = Reader::new(blocks, size.width as usize, size.height as usize);
  reader.meta = meta;

  // Restore reading progress if available.
  if let Some(ref key) = progress_key {
    let map = progress::load();
    if let Some(p) = map.get(key) {
      let max_offset = reader.total_lines().saturating_sub(1);
      reader.offset = p.offset.min(max_offset);
    }
  }

  let result = event_loop(&mut terminal, &mut reader);

  // Persist reading progress on clean exit.
  if let Some(ref key) = progress_key {
    let mut map = progress::load();
    map.insert(key.clone(), progress::ReaderProgress { offset: reader.offset });
    progress::save(&map);
  }

  disable_raw_mode()?;
  execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
  terminal.show_cursor()?;

  result
}

fn event_loop(
  terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
  reader: &mut Reader,
) -> Result<(), Box<dyn std::error::Error>> {
  loop {
    terminal.draw(|f| render::draw(f, reader))?;

    match event::read()? {
      Event::Key(key) => match reader.mode {
        Mode::Normal => {
          if handle_normal(reader, key.code, key.modifiers) {
            break;
          }
        }
        Mode::Search => handle_search(reader, key.code),
      },
      Event::Resize(w, h) => reader.resize(w as usize, h as usize),
      _ => {}
    }
  }
  Ok(())
}

fn handle_normal(reader: &mut Reader, code: KeyCode, mods: KeyModifiers) -> bool {
  match code {
    KeyCode::Char('q') | KeyCode::Esc => return true,
    KeyCode::Char('j') | KeyCode::Down => reader.nav_down(),
    KeyCode::Char('k') | KeyCode::Up => reader.nav_up(),
    KeyCode::Char('g') => reader.nav_top(),
    KeyCode::Char('G') => reader.nav_bottom(),
    KeyCode::Char('d') if mods.contains(KeyModifiers::CONTROL) => reader.nav_half_page_down(),
    KeyCode::Char('u') if mods.contains(KeyModifiers::CONTROL) => reader.nav_half_page_up(),
    KeyCode::Char('/') => reader.enter_search(),
    KeyCode::Char('n') => reader.search_next(),
    KeyCode::Char('N') => reader.search_prev(),
    KeyCode::Char(']') => reader.jump_next_section(),
    KeyCode::Char('[') => reader.jump_prev_section(),
    KeyCode::Char('t') => reader.toggle_toc(),
    KeyCode::Char('o') if mods.contains(KeyModifiers::CONTROL) => reader.nav_back(),
    _ => {}
  }
  false
}

fn handle_search(reader: &mut Reader, code: KeyCode) {
  match code {
    KeyCode::Esc => reader.cancel_search(),
    KeyCode::Enter => reader.confirm_search(),
    KeyCode::Backspace => {
      reader.search_query.pop();
      reader.update_search_matches();
    }
    KeyCode::Char(c) => {
      reader.search_query.push(c);
      reader.update_search_matches();
    }
    _ => {}
  }
}
