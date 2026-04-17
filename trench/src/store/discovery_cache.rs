use std::fs;
use std::path::PathBuf;

use crate::models::FeedItem;

pub fn path() -> Option<PathBuf> {
  let mut p = std::env::var_os("HOME").map(PathBuf::from)?;
  p.push(".config");
  p.push("trench");
  p.push("discovery_cache.json");
  Some(p)
}

pub fn load() -> Vec<FeedItem> {
  let path = match path() {
    Some(p) => p,
    None => return Vec::new(),
  };

  let bytes = match fs::read(&path) {
    Ok(b) => b,
    Err(_) => return Vec::new(),
  };

  serde_json::from_slice(&bytes).unwrap_or_default()
}

pub fn save(items: &[FeedItem]) {
  let path = match path() {
    Some(p) => p,
    None => return,
  };

  if let Some(parent) = path.parent() {
    let _ = fs::create_dir_all(parent);
  }

  if let Ok(json) = serde_json::to_vec_pretty(items) {
    let _ = fs::write(&path, json);
  }
}
