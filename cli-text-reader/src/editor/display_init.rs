use super::{core::Editor, runtime};
use crate::bookmarks::load_bookmarks;
use crate::config::load_config;
use crate::highlights::load_highlights;
use crate::progress::load_progress;
use crate::voice::playback::PlaybackController;

impl Editor {
  pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
    let config = load_config();

    self.show_highlighter = config.enable_line_highlighter.unwrap_or(true);
    self.show_cursor = config.show_cursor.unwrap_or(true);
    self.show_progress = config.show_progress.unwrap_or(true);

    // Initialise voice controller if an API key is configured
    if !config.elevenlabs_api_key.is_empty() {
      self.voice_controller = Some(PlaybackController::new(
        config.elevenlabs_api_key.clone(),
        config.voice_id.clone(),
      ));
    }

    // Check if tutorial should be shown
    let tutorial_enabled = config.enable_tutorial.unwrap_or(true);
    let tutorial_shown = config.tutorial_shown.unwrap_or(false);

    // Load bookmarks
    if let Ok(bookmark_data) = load_bookmarks(self.document_hash) {
      self.marks = bookmark_data.marks;
    }

    // Load highlights
    match load_highlights(&self.document_hash.to_string()) {
      Ok(highlight_data) => {
        self.highlights = highlight_data;
        self.debug_log(&format!(
          "Loaded {} highlights",
          self.highlights.highlights.len()
        ));
      }
      Err(e) => {
        self.debug_log_error(&format!("Failed to load highlights: {e}"));
      }
    }

    // Tutorial will be shown automatically on first launch if enabled

    // Note: Even with empty lines, we should allow the editor to run
    // so users can access the tutorial with :tutorial command

    let mut skip_first_center = false;
    match load_progress(self.document_hash) {
      Ok(progress) => {
        // Check if we have new viewport information
        if let (Some(viewport_offset), Some(saved_cursor_y)) =
          (progress.viewport_offset, progress.cursor_y)
        {
          // Use exact saved viewport state
          self.offset = viewport_offset;
          self.cursor_y = saved_cursor_y;
          self.debug_log(&format!(
            "Restored exact viewport state: offset={viewport_offset}, cursor_y={saved_cursor_y}"
          ));
        } else {
          // Fallback to old logic for backward compatibility
          let saved_line = progress.offset;
          let content_height = self.height.saturating_sub(1);
          let center_y = content_height / 2;

          // Try to center the saved line on screen
          if saved_line < center_y {
            // Line is near the top, can't center fully
            self.offset = 0;
            self.cursor_y = saved_line;
          } else if saved_line >= self.total_lines.saturating_sub(center_y) {
            // Line is near the bottom
            if self.total_lines > content_height {
              self.offset = self.total_lines - content_height;
              self.cursor_y = saved_line - self.offset;
            } else {
              self.offset = 0;
              self.cursor_y = saved_line;
            }
          } else {
            // Normal case - center the saved line
            self.offset = saved_line.saturating_sub(center_y);
            self.cursor_y = center_y;
          }
          self.debug_log(&format!(
            "Using fallback progress logic: line={saved_line}, offset={}, cursor_y={}", 
            self.offset, self.cursor_y
          ));
        }

        // Update tracking fields
        self.last_offset = progress.offset;
        self.last_saved_viewport_offset = self.offset;
        skip_first_center = true;
      }
      Err(e) => {
        self.debug_log(&format!("No progress found: {e}"));
        self.offset = 0;
        // cursor_y is already initialized to height/2 in the constructor
      }
    };

    // Show tutorial on first launch or start demo mode
    if self.tutorial_demo_mode {
      let demo_id = self.demo_id.unwrap_or(0); // Default to marketing demo if no ID specified
      self.debug_log(&format!("Starting demo mode with ID: {demo_id}"));
      self.start_demo_mode(demo_id);
    } else if tutorial_enabled && !tutorial_shown && !self.tutorial_demo_mode {
      self.debug_log("Showing interactive tutorial for first-time user");
      self.show_interactive_tutorial()?;
    }

    self.apply_initial_layout(skip_first_center);
    runtime::run(self)?;
    Ok(())
  }
}
