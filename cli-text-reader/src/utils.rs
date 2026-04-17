// Common utility functions for the hygg-reader text reader
use dirs::config_dir;
use std::path::PathBuf;

/// Get the base hygg-reader configuration directory, creating it if it doesn't exist
pub fn get_hygg_reader_config_dir()
-> Result<PathBuf, Box<dyn std::error::Error>> {
  let config_base = config_dir().ok_or("Unable to find config directory")?;

  // Log the config directory path for debugging
  crate::debug::debug_log(
    "config",
    &format!("Base config directory: {config_base:?}"),
  );

  let mut config_path = config_base;
  config_path.push("hygg-reader");

  // Log the full path and attempt to create it
  crate::debug::debug_log(
    "config",
    &format!("Creating hygg-reader config directory: {config_path:?}"),
  );

  match std::fs::create_dir_all(&config_path) {
    Ok(_) => {
      crate::debug::debug_log(
        "config",
        "Config directory created successfully",
      );
    }
    Err(e) => {
      crate::debug::debug_log(
        "config",
        &format!("Failed to create config directory: {e}"),
      );
      return Err(Box::new(e));
    }
  }

  Ok(config_path)
}

/// Get a file path within the hygg-reader config directory
pub fn get_hygg_reader_config_file(
  filename: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
  let mut path = get_hygg_reader_config_dir()?;
  path.push(filename);
  Ok(path)
}

/// Get a file path within a subdirectory of the hygg-reader config directory
pub fn get_hygg_reader_subdir_file(
  subdir: &str,
  filename: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
  let mut path = get_hygg_reader_config_dir()?;
  path.push(subdir);
  std::fs::create_dir_all(&path)?;
  path.push(filename);
  Ok(path)
}

/// Ensure a config file exists with default content
pub fn ensure_config_file_with_defaults(
  path: &std::path::Path,
  default_content: &str,
) -> Result<(), Box<dyn std::error::Error>> {
  if !path.exists() {
    std::fs::write(path, default_content)?;
  }
  Ok(())
}

/// Parse a boolean environment variable
pub fn parse_bool_env_var(var_name: &str) -> Option<bool> {
  std::env::var(var_name).ok().map(|val| val.to_lowercase() == "true")
}

/// Safe mutex lock acquisition with error mapping
#[allow(dead_code)]
pub fn safe_mutex_lock<T>(
  mutex: &std::sync::Mutex<T>,
) -> Result<std::sync::MutexGuard<'_, T>, String> {
  mutex.lock().map_err(|e| format!("Failed to acquire mutex lock: {e}"))
}

#[cfg(test)]
mod tests;
