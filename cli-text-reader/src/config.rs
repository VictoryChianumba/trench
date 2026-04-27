use crate::utils::{
  ensure_config_file_with_defaults, get_hygg_reader_config_file,
  parse_bool_env_var,
};
use std::fs;
use std::path::PathBuf;

#[derive(Default)]
pub struct AppConfig {
  pub enable_tutorial: Option<bool>,
  pub enable_line_highlighter: Option<bool>,
  pub show_cursor: Option<bool>,
  pub show_progress: Option<bool>,
  pub tutorial_shown: Option<bool>,
  // TTS / voice
  pub elevenlabs_api_key: String,
  pub voice_id: String,
  pub playback_speed: f32,
  /// "elevenlabs" | "say" | "piper" — empty means auto-select
  pub tts_provider: String,
  pub say_voice: String,
  pub piper_binary: String,
  pub piper_model: String,
}

fn get_config_env_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
  get_hygg_reader_config_file(".env")
}

fn ensure_config_file() -> Result<(), Box<dyn std::error::Error>> {
  let config_path = get_config_env_path()?;
  ensure_config_file_with_defaults(
    &config_path,
    "ENABLE_TUTORIAL=true\nENABLE_LINE_HIGHLIGHTER=true\nSHOW_CURSOR=true\nSHOW_PROGRESS=true\nTUTORIAL_SHOWN=false\n",
  )
}

pub fn load_config() -> AppConfig {
  let mut config = AppConfig::default();

  if let Ok(config_path) = get_config_env_path()
    && ensure_config_file().is_ok()
  {
    dotenvy::from_path(config_path).ok();
    config.enable_tutorial = parse_bool_env_var("ENABLE_TUTORIAL");
    config.enable_line_highlighter =
      parse_bool_env_var("ENABLE_LINE_HIGHLIGHTER");
    config.show_cursor = parse_bool_env_var("SHOW_CURSOR");
    config.show_progress = parse_bool_env_var("SHOW_PROGRESS");
    config.tutorial_shown = parse_bool_env_var("TUTORIAL_SHOWN");
    config.elevenlabs_api_key =
      std::env::var("ELEVENLABS_API_KEY").unwrap_or_default();
    config.voice_id = std::env::var("VOICE_ID").unwrap_or_default();
    config.playback_speed = std::env::var("PLAYBACK_SPEED")
      .ok()
      .and_then(|v| v.parse().ok())
      .unwrap_or(1.0);
    config.tts_provider =
      std::env::var("TTS_PROVIDER").unwrap_or_default();
    config.say_voice = std::env::var("SAY_VOICE").unwrap_or_default();
    config.piper_binary =
      std::env::var("PIPER_BINARY").unwrap_or_default();
    config.piper_model =
      std::env::var("PIPER_MODEL").unwrap_or_default();
  }

  config
}

/// Parse the .env file directly into key=value pairs **without** mutating the
/// process environment. Used by `save_config` to read existing values for
/// merge without the `dotenvy` side-effect.
fn read_raw_config() -> std::collections::HashMap<String, String> {
  let path = match get_config_env_path() {
    Ok(p) => p,
    Err(_) => return Default::default(),
  };
  let text = match fs::read_to_string(&path) {
    Ok(t) => t,
    Err(_) => return Default::default(),
  };
  text
    .lines()
    .filter_map(|line| {
      let line = line.trim();
      if line.starts_with('#') || !line.contains('=') {
        return None;
      }
      let mut parts = line.splitn(2, '=');
      let key = parts.next()?.trim().to_string();
      let val = parts.next().unwrap_or("").trim().to_string();
      Some((key, val))
    })
    .collect()
}

pub fn save_config(
  config: &AppConfig,
) -> Result<(), Box<dyn std::error::Error>> {
  let config_path = get_config_env_path()?;

  // Read existing values directly from the file — do NOT call load_config(),
  // which would call dotenvy::from_path() and mutate the process environment.
  let existing = read_raw_config();
  let existing_bool = |key: &str, default: bool| -> bool {
    existing
      .get(key)
      .and_then(|v| crate::utils::parse_bool_env_var_from_str(v))
      .unwrap_or(default)
  };
  let existing_str =
    |key: &str| -> String { existing.get(key).cloned().unwrap_or_default() };
  let existing_f32 = |key: &str, default: f32| -> f32 {
    existing.get(key).and_then(|v| v.parse().ok()).unwrap_or(default)
  };

  let enable_tutorial = config
    .enable_tutorial
    .unwrap_or_else(|| existing_bool("ENABLE_TUTORIAL", true));
  let enable_line_highlighter = config
    .enable_line_highlighter
    .unwrap_or_else(|| existing_bool("ENABLE_LINE_HIGHLIGHTER", true));
  let show_cursor =
    config.show_cursor.unwrap_or_else(|| existing_bool("SHOW_CURSOR", true));
  let show_progress = config
    .show_progress
    .unwrap_or_else(|| existing_bool("SHOW_PROGRESS", true));
  let tutorial_shown = config
    .tutorial_shown
    .unwrap_or_else(|| existing_bool("TUTORIAL_SHOWN", false));
  let elevenlabs_api_key = if config.elevenlabs_api_key.is_empty() {
    existing_str("ELEVENLABS_API_KEY")
  } else {
    config.elevenlabs_api_key.clone()
  };
  let voice_id = if config.voice_id.is_empty() {
    existing_str("VOICE_ID")
  } else {
    config.voice_id.clone()
  };
  let playback_speed = if config.playback_speed == 0.0 {
    existing_f32("PLAYBACK_SPEED", 1.0)
  } else {
    config.playback_speed
  };

  let content = format!(
    "ENABLE_TUTORIAL={enable_tutorial}\nENABLE_LINE_HIGHLIGHTER={enable_line_highlighter}\nSHOW_CURSOR={show_cursor}\nSHOW_PROGRESS={show_progress}\nTUTORIAL_SHOWN={tutorial_shown}\nELEVENLABS_API_KEY={elevenlabs_api_key}\nVOICE_ID={voice_id}\nPLAYBACK_SPEED={playback_speed:.1}\n"
  );

  fs::write(&config_path, content)?;
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    let _ =
      fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600));
  }
  Ok(())
}
