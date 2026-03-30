use std::path::PathBuf;

use crate::Note;

fn notes_dir() -> PathBuf {
    let base = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join(".config").join("tentative").join("notes")
}

fn note_path(article_id: &str) -> PathBuf {
    notes_dir().join(format!("{article_id}.json"))
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

pub fn load_note(article_id: &str) -> Option<Note> {
    let path = note_path(article_id);
    let bytes = std::fs::read(&path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn save_note(note: &Note) -> anyhow::Result<()> {
    let dir = notes_dir();
    std::fs::create_dir_all(&dir)?;
    let bytes = serde_json::to_vec_pretty(note)?;
    std::fs::write(note_path(&note.article_id), bytes)?;
    Ok(())
}

pub fn delete_note(article_id: &str) -> anyhow::Result<()> {
    let path = note_path(article_id);
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}
