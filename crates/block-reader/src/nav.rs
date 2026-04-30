use crate::state::{Mode, Reader};

impl Reader {
  pub fn nav_down(&mut self) {
    let ch = self.content_height();
    let total = self.total_lines();
    if self.offset + self.cursor_y + 1 >= total {
      return;
    }
    if self.cursor_y + 1 < ch {
      self.cursor_y += 1;
    } else {
      self.offset += 1;
    }
  }

  pub fn nav_up(&mut self) {
    if self.cursor_y > 0 {
      self.cursor_y -= 1;
    } else if self.offset > 0 {
      self.offset -= 1;
    }
  }

  pub fn nav_top(&mut self) {
    self.offset = 0;
    self.cursor_y = 0;
  }

  pub fn nav_bottom(&mut self) {
    let total = self.total_lines();
    let ch = self.content_height();
    if total > ch {
      self.offset = total - ch;
      self.cursor_y = ch - 1;
    } else {
      self.offset = 0;
      self.cursor_y = total.saturating_sub(1);
    }
  }

  pub fn nav_half_page_down(&mut self) {
    let step = self.content_height() / 2;
    for _ in 0..step {
      self.nav_down();
    }
  }

  pub fn nav_half_page_up(&mut self) {
    let step = self.content_height() / 2;
    for _ in 0..step {
      self.nav_up();
    }
  }

  pub fn search_next(&mut self) {
    if self.search_matches.is_empty() {
      return;
    }
    self.search_idx = (self.search_idx + 1) % self.search_matches.len();
    let idx = self.search_idx;
    self.jump_to_match(idx);
  }

  pub fn search_prev(&mut self) {
    if self.search_matches.is_empty() {
      return;
    }
    self.search_idx = if self.search_idx == 0 {
      self.search_matches.len() - 1
    } else {
      self.search_idx - 1
    };
    let idx = self.search_idx;
    self.jump_to_match(idx);
  }

  pub fn enter_search(&mut self) {
    self.mode = Mode::Search;
    self.search_query.clear();
    self.search_matches.clear();
  }

  pub fn confirm_search(&mut self) {
    self.mode = Mode::Normal;
    if !self.search_matches.is_empty() {
      self.push_nav_mark();
      let idx = self.search_idx;
      self.jump_to_match(idx);
    }
  }

  pub fn cancel_search(&mut self) {
    self.mode = Mode::Normal;
    self.search_query.clear();
    self.search_matches.clear();
  }

  pub fn jump_next_section(&mut self) {
    let cur = self.current_line();
    let target = self.sections.iter().find(|s| s.0 > cur).map(|s| s.0);
    if let Some(line) = target {
      self.push_nav_mark();
      self.offset = line;
      self.cursor_y = 0;
    }
  }

  pub fn jump_prev_section(&mut self) {
    let cur = self.current_line();
    let target = self.sections.iter().rfind(|s| s.0 < cur).map(|s| s.0);
    if let Some(line) = target {
      self.push_nav_mark();
      self.offset = line;
      self.cursor_y = 0;
    }
  }
}
