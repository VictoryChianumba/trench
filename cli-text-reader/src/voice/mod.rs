pub mod elevenlabs;
pub mod macos_say;
pub mod piper;
pub mod playback;
pub mod provider;
pub mod stream_buffer;

pub use playback::{PlaybackCommand, PlaybackController, PlaybackStatus};
pub use provider::TtsProvider;

use std::time::Instant;

/// Snapshot of what the background thread is currently playing.
/// Updated each time a new chunk starts, so the editor can track word position.
pub struct VoicePlayingInfo {
  pub doc_start_line: usize,
  pub doc_end_line: usize,
  /// Wall-clock instant when the current chunk began playing.
  pub started_at: Instant,
  /// Total chars spoken in all *completed* chunks before this one.
  pub chars_before_chunk: usize,
}

/// Split `text` into chunks no larger than `max_chars`, respecting blank-line
/// paragraph boundaries.  Adjacent short paragraphs are merged.
pub fn chunk_paragraphs(text: &str) -> Vec<String> {
  const MAX: usize = 4500;
  let mut chunks: Vec<String> = Vec::new();
  let mut current = String::new();

  for para in text.split("\n\n") {
    let para = para.trim();
    if para.is_empty() {
      continue;
    }
    if current.is_empty() {
      current.push_str(para);
    } else if current.len() + 2 + para.len() <= MAX {
      current.push_str("\n\n");
      current.push_str(para);
    } else {
      chunks.push(std::mem::take(&mut current));
      current.push_str(para);
    }
  }
  if !current.is_empty() {
    chunks.push(current);
  }

  // Fallback: text with no blank lines — split at MAX chars on whitespace
  if chunks.is_empty() {
    let text = text.trim();
    let mut start = 0;
    while start < text.len() {
      let end = (start + MAX).min(text.len());
      let end = if end < text.len() {
        text[start..end]
          .rfind(char::is_whitespace)
          .map(|i| start + i + 1)
          .unwrap_or(end)
      } else {
        end
      };
      chunks.push(text[start..end].to_string());
      start = end;
    }
  }

  chunks
}

/// Build a `TtsProvider` from config, with automatic fallback:
/// explicit `TTS_PROVIDER` → ElevenLabs if key present → macOS `say`.
pub fn make_provider(
  config: &crate::config::AppConfig,
) -> Box<dyn TtsProvider> {
  let provider_name = if config.tts_provider.is_empty() {
    if !config.elevenlabs_api_key.is_empty() {
      "elevenlabs"
    } else {
      "say"
    }
  } else {
    config.tts_provider.as_str()
  };

  match provider_name {
    "elevenlabs" if !config.elevenlabs_api_key.is_empty() => {
      Box::new(elevenlabs::ElevenLabsService::new(
        config.elevenlabs_api_key.clone(),
        config.voice_id.clone(),
      ))
    }
    "piper" => Box::new(piper::PiperProvider {
      binary: config.piper_binary.clone(),
      model: config.piper_model.clone(),
    }),
    _ => {
      let voice = if config.say_voice.is_empty() {
        "Samantha".to_string()
      } else {
        config.say_voice.clone()
      };
      Box::new(macos_say::MacOsSayProvider { voice })
    }
  }
}
