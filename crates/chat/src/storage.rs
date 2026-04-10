use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use crate::{ChatIndex, ChatSession, ChatSessionMeta};

fn chats_dir() -> PathBuf {
    let base = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("tentative")
        .join("chats");
    base
}

fn index_path() -> PathBuf {
    chats_dir().join("index.json")
}

fn session_path(id: &str) -> PathBuf {
    chats_dir().join(format!("{id}.json"))
}

fn ensure_dir() -> Result<()> {
    let dir = chats_dir();
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }
    Ok(())
}

pub fn load_index() -> ChatIndex {
    let path = index_path();
    if !path.exists() {
        return ChatIndex {
            sessions: Vec::new(),
            default_provider: "claude".to_string(),
        };
    }
    let data = fs::read_to_string(&path).unwrap_or_default();
    serde_json::from_str(&data).unwrap_or(ChatIndex {
        sessions: Vec::new(),
        default_provider: "claude".to_string(),
    })
}

pub fn save_index(index: &ChatIndex) -> Result<()> {
    ensure_dir()?;
    let data = serde_json::to_string_pretty(index)?;
    fs::write(index_path(), data)?;
    Ok(())
}

pub fn load_session(id: &str) -> Option<ChatSession> {
    let path = session_path(id);
    if !path.exists() {
        return None;
    }
    let data = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn save_session(session: &ChatSession) -> Result<()> {
    ensure_dir()?;
    let data = serde_json::to_string_pretty(session)?;
    fs::write(session_path(&session.id), data)?;
    Ok(())
}

pub fn delete_session(id: &str) -> Result<()> {
    let path = session_path(id);
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub fn create_session(title: String, provider: Option<String>) -> ChatSession {
    let now = Utc::now();
    ChatSession {
        id: Uuid::new_v4().to_string(),
        title,
        created_at: now,
        updated_at: now,
        messages: Vec::new(),
        provider,
        total_input_tokens: 0,
        total_output_tokens: 0,
    }
}

pub fn session_to_meta(session: &ChatSession) -> ChatSessionMeta {
    ChatSessionMeta {
        id: session.id.clone(),
        title: session.title.clone(),
        created_at: session.created_at,
        updated_at: session.updated_at,
        provider: session.provider.clone(),
    }
}
