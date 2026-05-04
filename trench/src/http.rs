use std::io::Read;
use std::time::Duration;

pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
pub const MAX_BODY_BYTES: u64 = 10 * 1024 * 1024; // 10 MB

/// Build a blocking `reqwest` client with the standard timeout.
pub fn client() -> reqwest::blocking::Client {
  reqwest::blocking::Client::builder()
    .timeout(REQUEST_TIMEOUT)
    .build()
    .expect("failed to build HTTP client")
}

/// Read a response body up to `MAX_BODY_BYTES`. Returns an error if the body
/// exceeds the limit or cannot be decoded as UTF-8.
pub fn read_body(resp: reqwest::blocking::Response) -> Result<String, String> {
  let mut limited = resp.take(MAX_BODY_BYTES + 1);
  let mut buf = Vec::new();
  limited.read_to_end(&mut buf).map_err(|e| format!("body read error: {e}"))?;
  if buf.len() as u64 > MAX_BODY_BYTES {
    return Err(format!(
      "response body exceeds {} MB limit",
      MAX_BODY_BYTES / 1024 / 1024
    ));
  }
  String::from_utf8(buf).map_err(|e| format!("body encoding error: {e}"))
}
