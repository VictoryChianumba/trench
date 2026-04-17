use crate::models::FeedItem;

pub enum FetchMessage {
  Items(Vec<FeedItem>),
  SourceComplete(String),
  SourceError(String, String),
  AllComplete,
}
