use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::Note;

fn notes_dir() -> PathBuf {
  dirs::config_dir()
    .unwrap_or_else(|| PathBuf::from("."))
    .join("trench")
    .join("notes")
}

fn note_path(note_id: &str) -> PathBuf {
  notes_dir().join(format!("{note_id}.json"))
}

/// Crash-safe write: writes to `<path>.tmp`, fsyncs, then renames onto `path`.
/// A panic, SIGINT, or power loss either leaves the original unchanged or
/// replaces it atomically — never truncated. Notes are user-authored content,
/// so torn writes are particularly costly here.
fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
  let mut tmp_name: OsString = path.as_os_str().to_owned();
  tmp_name.push(".tmp");
  let tmp_path = PathBuf::from(tmp_name);
  let _ = std::fs::remove_file(&tmp_path);
  {
    let mut f = std::fs::File::create(&tmp_path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
  }
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(
      &tmp_path,
      std::fs::Permissions::from_mode(0o600),
    );
  }
  std::fs::rename(&tmp_path, path)
}

pub fn load_all_notes() -> anyhow::Result<Vec<Note>> {
  let dir = notes_dir();
  if !dir.exists() {
    return Ok(Vec::new());
  }
  let mut notes = Vec::new();
  for entry in std::fs::read_dir(&dir)? {
    let entry = entry?;
    let path = entry.path();
    if path.extension().and_then(|e| e.to_str()) == Some("json") {
      if let Ok(bytes) = std::fs::read(&path) {
        if let Ok(note) = serde_json::from_slice::<Note>(&bytes) {
          notes.push(note);
        } else {
          log::warn!("Failed to parse note at {}", path.display());
        }
      }
    }
  }
  Ok(notes)
}

pub fn load_note(note_id: &str) -> Option<Note> {
  let path = note_path(note_id);
  let bytes = std::fs::read(&path).ok()?;
  serde_json::from_slice(&bytes).ok()
}

pub fn save_note(note: &Note) -> anyhow::Result<()> {
  let dir = notes_dir();
  std::fs::create_dir_all(&dir)?;
  let bytes = serde_json::to_vec_pretty(note)?;
  atomic_write(&note_path(&note.note_id), &bytes)?;
  Ok(())
}

pub fn delete_note(note_id: &str) -> anyhow::Result<()> {
  let path = note_path(note_id);
  if path.exists() {
    std::fs::remove_file(&path)?;
  }
  Ok(())
}
