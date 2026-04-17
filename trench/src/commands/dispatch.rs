use crate::app::{App, FeedTab};
use crate::commands::parser::SlashCommandInvocation;

pub fn dispatch_slash_command(app: &mut App, cmd: SlashCommandInvocation) {
  match cmd {
    SlashCommandInvocation::ClearChat => {
      app.clear_chat_messages();
      app.status_message = Some("Cleared chat session".to_string());
    }
    SlashCommandInvocation::Discover { topic } => {
      if topic.is_empty() {
        app.push_chat_assistant_message("Usage: /discover TOPIC".to_string());
      } else {
        crate::workflows::discover::start(app, topic);
      }
    }
    SlashCommandInvocation::ClearDiscoveries => {
      app.discovery_items.clear();
      app.discovery_selected_index = 0;
      app.discovery_list_offset = 0;
      app.discovery_plan = None;
      app.discovery_loading = false;
      crate::store::discovery_cache::save(&app.discovery_items);
      app.push_chat_assistant_message("Cleared discovery results.".to_string());
      app.status_message = Some("Cleared discovery results".to_string());
      if app.feed_tab == FeedTab::Discoveries {
        app.reset_active_feed_position();
      }
    }
    SlashCommandInvocation::AddArxivCategory { category } => {
      if category.is_empty() {
        app.push_chat_assistant_message(
          "Usage: /add ARXIV_CATEGORY".to_string(),
        );
      } else if app
        .config
        .sources
        .arxiv_categories
        .iter()
        .any(|c| c == &category)
      {
        app.push_chat_assistant_message(format!(
          "{category} is already configured."
        ));
      } else {
        app.config.sources.arxiv_categories.push(category.clone());
        app.config.save();
        app.push_chat_assistant_message(format!(
          "Added arXiv category: {category}"
        ));
      }
    }
    SlashCommandInvocation::AddFeed { url } => {
      if !crate::discovery::ai_query::is_http_url(&url) {
        app.push_chat_assistant_message(
          "Usage: /add-feed http(s)://feed-url".to_string(),
        );
      } else if app.config.sources.custom_feeds.iter().any(|f| f.url == url) {
        app.push_chat_assistant_message(
          "Feed is already configured.".to_string(),
        );
      } else {
        app.config.sources.custom_feeds.push(crate::config::CustomFeed {
          url: url.clone(),
          name: feed_name_from_url(&url),
          feed_type: "rss".to_string(),
        });
        app.config.save();
        app.push_chat_assistant_message(format!("Added RSS feed: {url}"));
      }
    }
    SlashCommandInvocation::Trending { .. } => {
      app.push_chat_assistant_message(
        "/trending is planned but not implemented yet.".to_string(),
      );
    }
    SlashCommandInvocation::Watch { .. } => {
      app.push_chat_assistant_message(
        "/watch is planned but not implemented yet.".to_string(),
      );
    }
    SlashCommandInvocation::Unknown { raw } => {
      app.push_chat_assistant_message(format!("Unknown slash command: {raw}"));
    }
  }
}

fn feed_name_from_url(url: &str) -> String {
  url
    .trim_start_matches("https://")
    .trim_start_matches("http://")
    .split('/')
    .next()
    .unwrap_or("feed")
    .to_string()
}
