use crate::utils::{
  ensure_config_file_with_defaults, get_hygg_config_file, parse_bool_env_var,
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
}

fn get_config_env_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
  get_hygg_config_file(".env")
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
    config.voice_id =
      std::env::var("VOICE_ID").unwrap_or_default();
    config.playback_speed = std::env::var("PLAYBACK_SPEED")
      .ok()
      .and_then(|v| v.parse().ok())
      .unwrap_or(1.0);
  }

  config
}

pub fn save_config(
  config: &AppConfig,
) -> Result<(), Box<dyn std::error::Error>> {
  let config_path = get_config_env_path()?;

  let existing_config = load_config();

  let enable_tutorial =
    config.enable_tutorial.or(existing_config.enable_tutorial).unwrap_or(true);
  let enable_line_highlighter = config
    .enable_line_highlighter
    .or(existing_config.enable_line_highlighter)
    .unwrap_or(true);
  let show_cursor =
    config.show_cursor.or(existing_config.show_cursor).unwrap_or(true);
  let show_progress =
    config.show_progress.or(existing_config.show_progress).unwrap_or(true);
  let tutorial_shown =
    config.tutorial_shown.or(existing_config.tutorial_shown).unwrap_or(false);

  let elevenlabs_api_key = if config.elevenlabs_api_key.is_empty() {
    existing_config.elevenlabs_api_key.clone()
  } else {
    config.elevenlabs_api_key.clone()
  };
  let voice_id = if config.voice_id.is_empty() {
    existing_config.voice_id.clone()
  } else {
    config.voice_id.clone()
  };
  let playback_speed = if config.playback_speed == 0.0 {
    existing_config.playback_speed
  } else {
    config.playback_speed
  };

  let content = format!(
    "ENABLE_TUTORIAL={enable_tutorial}\nENABLE_LINE_HIGHLIGHTER={enable_line_highlighter}\nSHOW_CURSOR={show_cursor}\nSHOW_PROGRESS={show_progress}\nTUTORIAL_SHOWN={tutorial_shown}\nELEVENLABS_API_KEY={elevenlabs_api_key}\nVOICE_ID={voice_id}\nPLAYBACK_SPEED={playback_speed:.1}\n"
  );

  fs::write(config_path, content)?;
  Ok(())
}
