use serde::Deserialize;

// ── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum NodeType {
  Dir,
  File,
}

#[derive(Debug, Clone)]
pub struct TreeNode {
  pub name: String,
  pub path: String,
  pub node_type: NodeType,
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Returns the default branch name for the given repo.
pub fn get_default_branch(
  owner: &str,
  repo: &str,
  token: &str,
) -> Result<String, String> {
  let url = format!("https://api.github.com/repos/{owner}/{repo}");
  let resp: RepoInfo = get_json(&url, token)?;
  Ok(resp.default_branch)
}

/// Returns the directory listing for `path` (empty string for root).
/// Directories come before files; both groups are sorted alphabetically.
pub fn fetch_tree_dir(
  owner: &str,
  repo: &str,
  branch: &str,
  path: &str,
  token: &str,
) -> Result<Vec<TreeNode>, String> {
  let path_seg =
    if path.is_empty() { String::new() } else { format!("/{}", path) };
  let url = format!(
    "https://api.github.com/repos/{owner}/{repo}/contents{path_seg}?ref={branch}"
  );

  let items: Vec<ContentItem> = get_json(&url, token)?;

  let mut nodes: Vec<TreeNode> = items
    .into_iter()
    .map(|item| TreeNode {
      name: item.name,
      path: item.path,
      node_type: if item.item_type == "dir" {
        NodeType::Dir
      } else {
        NodeType::File
      },
    })
    .collect();

  nodes.sort_by(|a, b| {
    let a_dir = matches!(a.node_type, NodeType::Dir);
    let b_dir = matches!(b.node_type, NodeType::Dir);
    b_dir.cmp(&a_dir).then_with(|| a.name.cmp(&b.name))
  });

  Ok(nodes)
}

/// Returns the raw UTF-8 text of a file.
/// Returns an error for binary files or files that exceed the GitHub 1 MB limit.
pub fn fetch_file(
  owner: &str,
  repo: &str,
  path: &str,
  token: &str,
) -> Result<String, String> {
  let url =
    format!("https://api.github.com/repos/{owner}/{repo}/contents/{path}");
  let resp: FileContent = get_json(&url, token)?;

  if resp.encoding.as_deref() != Some("base64") {
    return Err(format!(
      "Unexpected encoding: {}",
      resp.encoding.unwrap_or_default()
    ));
  }

  let clean = resp.content.replace('\n', "").replace('\r', "");
  let bytes = base64_decode(&clean)?;
  String::from_utf8(bytes)
    .map_err(|_| "Binary file — cannot display".to_string())
}

// ── HTTP helpers ─────────────────────────────────────────────────────────────

fn get_json<T: for<'de> Deserialize<'de>>(
  url: &str,
  token: &str,
) -> Result<T, String> {
  let client = reqwest::blocking::Client::new();
  let mut req = client
    .get(url)
    .header("User-Agent", "trench/0.1")
    .header("Accept", "application/vnd.github+json")
    .header("X-GitHub-Api-Version", "2022-11-28");

  if !token.is_empty() {
    req = req.header("Authorization", format!("Bearer {token}"));
  }

  let resp = req.send().map_err(|e| format!("HTTP error: {e}"))?;

  if !resp.status().is_success() {
    return Err(format!("GitHub API: HTTP {}", resp.status()));
  }

  resp.json::<T>().map_err(|e| format!("JSON parse error: {e}"))
}

// ── Serde types ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct RepoInfo {
  default_branch: String,
}

#[derive(Deserialize)]
struct ContentItem {
  name: String,
  path: String,
  #[serde(rename = "type")]
  item_type: String,
}

#[derive(Deserialize)]
struct FileContent {
  content: String,
  encoding: Option<String>,
}

// ── Base64 decoder ───────────────────────────────────────────────────────────

fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
  let mut map = [255u8; 256];
  for (i, &c) in
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
      .iter()
      .enumerate()
  {
    map[c as usize] = i as u8;
  }

  let bytes: Vec<u8> =
    s.bytes().filter(|&b| b != b'=' && map[b as usize] != 255).collect();

  let mut out = Vec::with_capacity(bytes.len() * 3 / 4 + 3);
  let mut i = 0;

  while i + 4 <= bytes.len() {
    let n = (map[bytes[i] as usize] as u32) << 18
      | (map[bytes[i + 1] as usize] as u32) << 12
      | (map[bytes[i + 2] as usize] as u32) << 6
      | (map[bytes[i + 3] as usize] as u32);
    out.push((n >> 16) as u8);
    out.push((n >> 8) as u8);
    out.push(n as u8);
    i += 4;
  }

  match bytes.len() - i {
    2 => {
      let n = (map[bytes[i] as usize] as u32) << 18
        | (map[bytes[i + 1] as usize] as u32) << 12;
      out.push((n >> 16) as u8);
    }
    3 => {
      let n = (map[bytes[i] as usize] as u32) << 18
        | (map[bytes[i + 1] as usize] as u32) << 12
        | (map[bytes[i + 2] as usize] as u32) << 6;
      out.push((n >> 16) as u8);
      out.push((n >> 8) as u8);
    }
    _ => {}
  }

  Ok(out)
}
