pub mod cache;
pub mod discovery_cache;
pub mod enrichment_cache;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::models::WorkflowState;

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
  }
}
