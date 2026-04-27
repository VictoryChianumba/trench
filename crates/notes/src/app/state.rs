use std::{fs::File, io::BufWriter, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::sorter::Sorter;

const STATE_FILE_NAME: &str = "state.json";

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct AppState {
  pub sorter: Sorter,
  pub full_screen: bool,
}

impl AppState {
  fn state_path() -> PathBuf {
    dirs::config_dir()
      .unwrap_or_else(|| PathBuf::from("."))
      .join("trench")
      .join("notes")
      .join(STATE_FILE_NAME)
  }

  pub fn load() -> anyhow::Result<Self> {
    let path = Self::state_path();
    if !path.exists() {
      return Ok(AppState::default());
    }
    let file = File::open(&path)
      .map_err(|err| anyhow::anyhow!("Failed to open state file: {err}"))?;
    let state = serde_json::from_reader(file)
      .map_err(|err| anyhow::anyhow!("Failed to parse state file: {err}"))?;
    Ok(state)
  }

  pub fn save(&self) -> anyhow::Result<()> {
    let path = Self::state_path();
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent)?;
    }
    let file = File::create(&path)?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, self)?;
    Ok(())
  }
}
