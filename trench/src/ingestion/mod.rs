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
