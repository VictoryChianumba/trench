use std::collections::HashSet;
use std::process::{Command, Stdio};
use std::time::Duration;

// Command output structure
pub struct CommandOutput {
  pub stdout: String,
  pub stderr: String,
  pub status: std::process::ExitStatus,
}

// Secure command structure for validated commands
#[derive(Debug)]
pub struct SecureCommand {
  pub program: String,
  pub args: Vec<String>,
}

// Parse and validate command using whitelist approach
pub fn parse_secure_command(cmd: &str) -> Result<SecureCommand, String> {
  let cmd = cmd.trim();
  if cmd.is_empty() {
    return Err("Empty command".to_string());
  }

  let cmd_to_parse = cmd;

  // Whitelist of allowed commands — read-only filesystem and text operations only.
  // Excluded intentionally:
  //   env/printenv/history — expose secrets from the process environment and shell history
  //   curl/wget/ping/dig/nslookup — make outbound network connections
  //   tar/zip/unzip/gzip/gunzip/zcat — can write arbitrary files during extraction
  //   echo/printf — can be chained with redirects to write files in some contexts
  //   PowerShell entries — Windows translation is a separate concern; do not expand attack surface here
  let allowed_commands: HashSet<&str> = [
    // Directory listing and path navigation
    "ls",
    "pwd",
    "find",
    "locate",
    "which",
    "whereis",
    // File viewing (core functionality for text reader)
    "cat",
    "less",
    "more",
    "head",
    "tail",
    "file",
    "stat",
    "wc",
    "nl",
    // Text processing (read-only)
    "grep",
    "awk",
    "sed",
    "sort",
    "uniq",
    "cut",
    "tr",
    "fmt",
    "fold",
    // System information (read-only, no secrets)
    "date",
    "uptime",
    "whoami",
    "id",
    "uname",
    "hostname",
    "df",
    "free",
    "ps",
    // Path utilities
    "basename",
    "dirname",
    "realpath",
    "readlink",
  ]
  .iter()
  .cloned()
  .collect();

  // Split command into parts
  let parts: Vec<&str> = cmd_to_parse.split_whitespace().collect();
  if parts.is_empty() {
    return Err("Invalid command".to_string());
  }

  let program = parts[0];

  // Check if command is whitelisted
  if !allowed_commands.contains(program) {
    return Err(format!("Command '{program}' is not allowed"));
  }

  // Reject shell metacharacters in arguments. We do not use a shell to execute
  // commands, but some commands (awk, sed) interpret these themselves.
  let dangerous_chars: &[char] =
    &['|', '&', ';', '`', '$', '(', ')', '<', '>', '\\', '*', '?'];

  for arg in &parts[1..] {
    if arg.chars().any(|c| dangerous_chars.contains(&c)) {
      return Err(format!("Argument contains dangerous characters: {arg}"));
    }
    if arg.len() > 1000 {
      return Err("Argument too long (max 1000 characters)".to_string());
    }
  }

  if parts.len() > 50 {
    return Err("Too many arguments (max 50)".to_string());
  }

  Ok(SecureCommand {
    program: program.to_string(),
    args: parts[1..].iter().map(|s| s.to_string()).collect(),
  })
}

// Execute a validated command with timeout
pub fn execute_secure_command_with_timeout(
  secure_cmd: SecureCommand,
  timeout: Duration,
) -> Result<CommandOutput, String> {
  let mut child = Command::new(&secure_cmd.program)
    .args(&secure_cmd.args)
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .map_err(|e| {
      format!("Failed to execute command '{}': {}", secure_cmd.program, e)
    })?;

  // Wait for the command with timeout
  match child.wait_timeout(timeout) {
    Ok(Some(status)) => {
      // Command completed within timeout
      let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to read output: {e}"))?;

      Ok(CommandOutput {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        status,
      })
    }
    Ok(None) => {
      // Timeout occurred, kill the process
      let _ = child.kill();
      Err("Command timed out after 30 seconds".to_string())
    }
    Err(e) => Err(format!("Failed to wait for command: {e}")),
  }
}

// Extension trait for waiting with timeout
trait WaitTimeout {
  fn wait_timeout(
    &mut self,
    dur: Duration,
  ) -> std::io::Result<Option<std::process::ExitStatus>>;
}

impl WaitTimeout for std::process::Child {
  fn wait_timeout(
    &mut self,
    dur: Duration,
  ) -> std::io::Result<Option<std::process::ExitStatus>> {
    let start = std::time::Instant::now();

    loop {
      match self.try_wait()? {
        Some(status) => return Ok(Some(status)),
        None => {
          if start.elapsed() >= dur {
            return Ok(None);
          }
          std::thread::sleep(Duration::from_millis(100));
        }
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::parse_secure_command;

  #[test]
  fn test_allowed_commands() {
    let allowed = vec!["cat", "less", "head", "tail", "grep", "ls", "pwd"];
    for cmd in allowed {
      assert!(parse_secure_command(cmd).is_ok(), "{cmd} should be allowed");
    }
  }

  #[test]
  fn test_rejected_commands() {
    let rejected = vec!["rm", "sudo", "kill", "reboot"];
    for cmd in rejected {
      assert!(parse_secure_command(cmd).is_err(), "{cmd} should be rejected");
    }
  }

  #[test]
  fn test_dangerous_chars() {
    let dangerous =
      vec!["cat file; rm file", "echo `cmd`", "ls > file", "cmd | other"];
    for input in dangerous {
      assert!(
        parse_secure_command(input).is_err(),
        "{input} should be rejected"
      );
    }
  }
}
