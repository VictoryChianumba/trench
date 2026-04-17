use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

fn cache_path() -> Option<PathBuf> {
  let mut p = std::env::var_os("HOME").map(PathBuf::from)?;
  p.push(".config");
  p.push("trench");
  p.push("enrichment_cache.json");
  Some(p)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EnrichmentEntry {
  pub authors: Vec<String>,
  pub institution: String,
  pub citation_count: u32,
  pub fields_of_study: Vec<String>,
  pub cached_at: String,
}

pub fn load() -> HashMap<String, EnrichmentEntry> {
  let path = match cache_path() {
    Some(p) => p,
    None => return HashMap::new(),
  };
  let bytes = match fs::read(&path) {
    Ok(b) => b,
    Err(_) => return HashMap::new(),
  };
  let mut cache: HashMap<String, EnrichmentEntry> =
    serde_json::from_slice(&bytes).unwrap_or_default();
  // Invalidate entries with no field data so they are re-fetched once an API
  // key is configured.
  for entry in cache.values_mut() {
    if entry.fields_of_study.is_empty() {
      entry.cached_at = "1970-01-01".to_string();
    }
  }
  log::info!("enrichment_cache: loaded {} entries", cache.len());
  cache
}

pub fn save(cache: &HashMap<String, EnrichmentEntry>) {
  let path = match cache_path() {
    Some(p) => p,
    None => return,
  };
  if let Some(parent) = path.parent() {
    let _ = fs::create_dir_all(parent);
  }
  if let Ok(json) = serde_json::to_vec_pretty(cache) {
    if let Err(e) = fs::write(&path, json) {
      log::warn!("enrichment_cache: failed to save to {path:?} — {e}");
    }
  }
}

/// Returns true if the entry was cached more than 7 days ago.
pub fn is_stale(entry: &EnrichmentEntry, id: &str) -> bool {
  let (ty, tm, td) = today_ymd();
  match parse_ymd(&entry.cached_at) {
    Some((cy, cm, cd)) => {
      let stale = ymd_to_jdn(ty, tm, td) - ymd_to_jdn(cy, cm, cd) > 7;
      if stale {
        log::debug!(
          "enrichment_cache: stale entry for arXiv:{id} (cached_at={})",
          entry.cached_at
        );
      }
      stale
    }
    None => true,
  }
}

/// Today's date as "YYYY-MM-DD".
pub fn today_str() -> String {
  let (y, m, d) = today_ymd();
  format!("{y:04}-{m:02}-{d:02}")
}

// ---------------------------------------------------------------------------
// Date helpers (no external crates)
// ---------------------------------------------------------------------------

fn today_ymd() -> (i32, i32, i32) {
  let secs = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap_or_default()
    .as_secs() as i32;
  // Unix epoch = JDN 2440588
  jdn_to_ymd(secs / 86400 + 2440588)
}

fn jdn_to_ymd(jdn: i32) -> (i32, i32, i32) {
  let a = jdn + 32044;
  let b = (4 * a + 3) / 146097;
  let c = a - 146097 * b / 4;
  let d = (4 * c + 3) / 1461;
  let e = c - 1461 * d / 4;
  let m = (5 * e + 2) / 153;
  let day = e - (153 * m + 2) / 5 + 1;
  let month = m + 3 - 12 * (m / 10);
  let year = 100 * b + d - 4800 + m / 10;
  (year, month, day)
}

fn ymd_to_jdn(y: i32, m: i32, d: i32) -> i32 {
  let a = (14 - m) / 12;
  let y2 = y + 4800 - a;
  let m2 = m + 12 * a - 3;
  d + (153 * m2 + 2) / 5 + y2 * 365 + y2 / 4 - y2 / 100 + y2 / 400 - 32045
}

fn parse_ymd(s: &str) -> Option<(i32, i32, i32)> {
  let mut it = s.splitn(3, '-');
  let y: i32 = it.next()?.parse().ok()?;
  let m: i32 = it.next()?.parse().ok()?;
  // trim any trailing non-digit chars (e.g. 'T' from ISO 8601)
  let d_str = it.next()?;
  let d: i32 =
    d_str.trim_end_matches(|c: char| !c.is_ascii_digit()).parse().ok()?;
  Some((y, m, d))
}
