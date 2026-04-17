use crate::app::{App, FeedTab};

pub fn start(app: &mut App, topic: String) {
  let (tx, rx) = std::sync::mpsc::channel();
  app.discovery_rx = Some(rx);
  app.discovery_loading = true;
  app.feed_tab = FeedTab::Discoveries;
  app.reset_active_feed_position();
  crate::discovery::pipeline::spawn_discovery(
    topic.clone(),
    app.config.clone(),
    tx,
  );
  app.status_message = Some(format!("Discovering: {topic}"));
}
