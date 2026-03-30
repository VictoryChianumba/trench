use std::collections::HashMap;

use crate::provider::ChatProvider;

pub struct ProviderRegistry {
    providers: HashMap<String, Box<dyn ChatProvider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self { providers: HashMap::new() }
    }

    pub fn register(&mut self, name: impl Into<String>, provider: Box<dyn ChatProvider>) {
        self.providers.insert(name.into(), provider);
    }

    pub fn get(&self, name: &str) -> Option<&dyn ChatProvider> {
        self.providers.get(name).map(|p| p.as_ref())
    }

    pub fn names(&self) -> Vec<&str> {
        self.providers.keys().map(|k| k.as_str()).collect()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Parses an optional provider prefix from user input.
///
/// `"claude: what is X?"` → `(Some("claude"), "what is X?")`
/// `"what is X?"` → `(None, "what is X?")`
pub fn parse_provider_prefix(input: &str) -> (Option<String>, String) {
    // Look for a known-looking prefix: word characters followed by ": "
    if let Some(colon_pos) = input.find(": ") {
        let candidate = &input[..colon_pos];
        // Only treat it as a prefix if it's a single lowercase word (no spaces)
        if !candidate.is_empty() && candidate.chars().all(|c| c.is_alphanumeric() || c == '-') {
            let rest = input[colon_pos + 2..].to_string();
            return (Some(candidate.to_lowercase()), rest);
        }
    }
    (None, input.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_provider_prefix() {
        assert_eq!(
            parse_provider_prefix("claude: what is X?"),
            (Some("claude".to_string()), "what is X?".to_string())
        );
        assert_eq!(
            parse_provider_prefix("openai: hello"),
            (Some("openai".to_string()), "hello".to_string())
        );
        assert_eq!(
            parse_provider_prefix("what is X?"),
            (None, "what is X?".to_string())
        );
        assert_eq!(
            parse_provider_prefix("no colon here"),
            (None, "no colon here".to_string())
        );
    }
}
