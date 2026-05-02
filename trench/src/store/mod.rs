pub mod cache;
pub mod discovery_cache;
pub mod enrichment_cache;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::app::NotesTab;
use crate::models::WorkflowState;

/// Restrict a file to owner-read/write only (0o600). Best-effort on Unix.
pub(crate) fn set_private(path: &PathBuf) {
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
  }
  let _ = path; // suppress unused warning on non-Unix
}

fn state_path() -> Option<PathBuf> {
  let mut p = dirs_home()?;
  p.push(".config");
  p.push("trench");
  p.push("state.json");
  Some(p)
}

/// Best-effort home directory — uses $HOME, falls back to nothing.
fn dirs_home() -> Option<PathBuf> {
  std::env::var_os("HOME").map(PathBuf::from)
}

pub fn load() -> HashMap<String, WorkflowState> {
  let path = match state_path() {
    Some(p) => p,
    None => return HashMap::new(),
  };

  let bytes = match fs::read(&path) {
    Ok(b) => b,
    Err(_) => return HashMap::new(),
  };

  serde_json::from_slice(&bytes).unwrap_or_default()
}

pub fn save(state: &HashMap<String, WorkflowState>) {
  let path = match state_path() {
    Some(p) => p,
    None => return,
  };

  // Create parent dirs if needed — silently ignore failure
  if let Some(parent) = path.parent() {
    let _ = fs::create_dir_all(parent);
  }

  if let Ok(json) = serde_json::to_vec_pretty(state) {
    let _ = fs::write(&path, json);
    set_private(&path);
  }
}

// ── UI state (last_read, etc.) ─────────────────────────────────────────────

#[derive(serde::Serialize, serde::Deserialize, Default)]
pub struct UiState {
  pub last_read:        Option<String>,
  pub last_read_source: Option<String>,
  #[serde(default)]
  pub notes_tabs:       Vec<NotesTab>,
  #[serde(default)]
  pub notes_active_tab: usize,
}

fn ui_path() -> Option<PathBuf> {
  let mut p = dirs_home()?;
  p.push(".config");
  p.push("trench");
  p.push("ui.json");
  Some(p)
}

pub fn load_ui() -> UiState {
  let path = match ui_path() {
    Some(p) => p,
    None => return UiState::default(),
  };
  let bytes = match fs::read(&path) {
    Ok(b) => b,
    Err(_) => return UiState::default(),
  };
  serde_json::from_slice(&bytes).unwrap_or_default()
}

pub fn save_ui(state: &UiState) {
  let path = match ui_path() {
    Some(p) => p,
    None => return,
  };
  if let Some(parent) = path.parent() {
    let _ = fs::create_dir_all(parent);
  }
  if let Ok(json) = serde_json::to_vec_pretty(state) {
    let _ = fs::write(&path, json);
    set_private(&path);
  }
}
