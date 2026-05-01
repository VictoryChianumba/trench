pub mod arxiv;
pub mod block_fetch;
pub mod fulltext;
pub mod huggingface;
pub mod message;
pub mod rss;
pub mod semantic_scholar;

pub(super) fn collapse_whitespace(s: &str) -> String {
  s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn truncate_chars(s: &str, max: usize) -> String {
  let mut chars = s.chars();
  let mut out = String::new();
  let mut n = 0;
  for c in &mut chars {
    if n >= max {
      if chars.next().is_some() {
        out.push('…');
      }
      break;
    }
    out.push(c);
    n += 1;
  }
  out
}
