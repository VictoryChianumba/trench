use std::sync::mpsc;

use crate::config::Config;
use crate::discovery::{DiscoveryMessage, ai_query};

pub fn spawn_discovery(
  topic: String,
  config: Config,
  tx: mpsc::Sender<DiscoveryMessage>,
) {
  std::thread::spawn(move || match ai_query::run_ai_query(&topic, &config) {
    Ok(plan) => {
      let _ = tx.send(DiscoveryMessage::PlanReady(plan.clone()));

      for term in &plan.search_terms {
        match crate::ingestion::arxiv::search_query(term, 20) {
          Ok(items) if !items.is_empty() => {
            let _ = tx.send(DiscoveryMessage::Items(items));
          }
          Ok(_) => {}
          Err(e) => {
            log::warn!("discovery arxiv search failed for {term:?}: {e}");
          }
        }
      }

      let new_categories: Vec<String> = plan
        .arxiv_categories
        .iter()
        .filter(|cat| !config.sources.arxiv_categories.contains(*cat))
        .cloned()
        .collect();
      if !new_categories.is_empty() {
        match crate::ingestion::arxiv::fetch(&new_categories) {
          Ok(items) if !items.is_empty() => {
            let _ = tx.send(DiscoveryMessage::Items(items));
          }
          Ok(_) => {}
          Err(e) => {
            log::warn!("discovery arxiv category fetch failed: {e}");
          }
        }
      }

      if !plan.paper_ids.is_empty() {
        match crate::ingestion::arxiv::fetch_by_ids(&plan.paper_ids) {
          Ok(items) if !items.is_empty() => {
            let _ = tx.send(DiscoveryMessage::Items(items));
          }
          Ok(_) => {}
          Err(e) => {
            log::warn!("discovery arxiv id fetch failed: {e}");
          }
        }
      }

      let _ = tx.send(DiscoveryMessage::Complete);
    }
    Err(e) => {
      let _ = tx.send(DiscoveryMessage::Error(e));
    }
  });
}
