use anyhow::Result;

use crate::ChatMessage;

pub trait ChatProvider: Send + Sync {
    fn send(&self, messages: &[ChatMessage]) -> Result<String>;
    fn name(&self) -> &str;
}
