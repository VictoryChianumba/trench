use anyhow::Result;

use crate::ChatMessage;

pub struct ProviderResponse {
    pub content: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

pub trait ChatProvider: Send + Sync {
    fn send(&self, messages: &[ChatMessage]) -> Result<ProviderResponse>;
    fn name(&self) -> &str;
    fn model(&self) -> &str;
    fn context_window(&self) -> u64;
}
