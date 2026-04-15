use aho_corasick::AhoCorasick;

use crate::Note;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterCriterion {
  Tag(TagFilterOption),
  Title(String),
  Content(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagFilterOption {
  Tag(String),
  NoTags,
}

impl FilterCriterion {
  /// Checks if the note meets the criterion
  pub fn check_note(&self, note: &Note) -> bool {
    match self {
      FilterCriterion::Tag(TagFilterOption::Tag(tag)) => {
        note.tags.contains(tag)
      }
      FilterCriterion::Tag(TagFilterOption::NoTags) => note.tags.is_empty(),
      FilterCriterion::Title(search) => {
        // Use simple smart-case search for title
        if search.chars().any(|c| c.is_uppercase()) {
          note.article_title.contains(search)
        } else {
          note.article_title.to_lowercase().contains(search)
        }
      }
      FilterCriterion::Content(search) => {
        if search.chars().any(|c| c.is_uppercase()) {
          // Use simple search when pattern already has uppercase
          note.content.contains(search)
        } else {
          // Otherwise use case insensitive pattern matcher
          let ac = match AhoCorasick::builder()
            .ascii_case_insensitive(true)
            .build([&search])
          {
            Ok(ac) => ac,
            Err(err) => {
              log::error!(
                "Build AhoCorasick with pattern {search} failed with error: {err}"
              );
              return false;
            }
          };

          ac.find(&note.content).is_some()
        }
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use chrono::{TimeZone, Utc};

  use super::*;

  fn sample_note(tags: Vec<&str>) -> Note {
    Note {
      article_id: "1".into(),
      article_title: "Rust Search".into(),
      article_url: "https://example.com/1".into(),
      content: "Searching CONTENT with Mixed Case".into(),
      tags: tags.into_iter().map(String::from).collect(),
      created_at: Utc.with_ymd_and_hms(2024, 1, 2, 3, 4, 5).unwrap(),
      updated_at: Utc.with_ymd_and_hms(2024, 1, 2, 3, 4, 5).unwrap(),
    }
  }

  #[test]
  fn tag_checks_match_exactly() {
    let note = sample_note(vec!["rust", "tests"]);

    assert!(
      FilterCriterion::Tag(TagFilterOption::Tag(String::from("rust")))
        .check_note(&note)
    );
    assert!(
      !FilterCriterion::Tag(TagFilterOption::Tag(String::from("Rust")))
        .check_note(&note)
    );
  }

  #[test]
  fn no_tags_requires_empty_list() {
    let note = sample_note(vec![]);
    let tagged_note = sample_note(vec!["tag"]);

    assert!(FilterCriterion::Tag(TagFilterOption::NoTags).check_note(&note));
    assert!(
      !FilterCriterion::Tag(TagFilterOption::NoTags).check_note(&tagged_note)
    );
  }

  #[test]
  fn title_search_uses_smart_case() {
    let note = sample_note(vec!["tag"]);

    assert!(FilterCriterion::Title(String::from("rust")).check_note(&note));
    assert!(FilterCriterion::Title(String::from("Rust")).check_note(&note));
    assert!(!FilterCriterion::Title(String::from("SEARCH")).check_note(&note));
  }

  #[test]
  fn content_search_uses_smart_case() {
    let note = sample_note(vec!["tag"]);

    assert!(
      FilterCriterion::Content(String::from("content")).check_note(&note)
    );
    assert!(FilterCriterion::Content(String::from("Mixed")).check_note(&note));
    assert!(!FilterCriterion::Content(String::from("mixed")).check_note(
      &Note {
        article_id: "2".into(),
        article_title: note.article_title.clone(),
        article_url: note.article_url.clone(),
        content: "UPPERCASE ONLY".into(),
        tags: note.tags.clone(),
        created_at: note.created_at,
        updated_at: note.updated_at,
      }
    ));
  }
}
