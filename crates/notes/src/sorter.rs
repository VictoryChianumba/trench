use crate::Note;
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, fmt::Display};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum SortCriteria {
  Date,
  Title,
}

impl Display for SortCriteria {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      SortCriteria::Date => write!(f, "Date"),
      SortCriteria::Title => write!(f, "Title"),
    }
  }
}

impl SortCriteria {
  fn compare(&self, note1: &Note, note2: &Note, order: &SortOrder) -> Ordering {
    let ascending_ord = match self {
      SortCriteria::Date => note1.created_at.cmp(&note2.created_at),
      SortCriteria::Title => note1.article_title.cmp(&note2.article_title),
    };

    match order {
      SortOrder::Ascending => ascending_ord,
      SortOrder::Descending => ascending_ord.reverse(),
    }
  }

  pub fn iterator() -> impl Iterator<Item = SortCriteria> {
    use SortCriteria as S;

    // Static assertions to make sure all sort criteria are involved in the iterator
    if cfg!(debug_assertions) {
      match S::Date {
        S::Date => (),
        S::Title => (),
      };
    }

    [S::Date, S::Title].iter().copied()
  }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub enum SortOrder {
  Ascending,
  Descending,
}

impl Display for SortOrder {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      SortOrder::Ascending => write!(f, "Ascending"),
      SortOrder::Descending => write!(f, "Descending"),
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sorter {
  criteria: Vec<SortCriteria>,
  pub order: SortOrder,
}

impl Default for Sorter {
  fn default() -> Self {
    let criteria = vec![SortCriteria::Date, SortCriteria::Title];

    Self { criteria, order: SortOrder::Descending }
  }
}

impl Sorter {
  pub fn set_criteria(&mut self, criteria: Vec<SortCriteria>) {
    self.criteria = criteria;
  }

  pub fn get_criteria(&self) -> &[SortCriteria] {
    &self.criteria
  }

  pub fn sort(&self, note1: &Note, note2: &Note) -> Ordering {
    self
      .criteria
      .iter()
      .map(|cr| cr.compare(note1, note2, &self.order))
      .find(|cmp| matches!(cmp, Ordering::Less | Ordering::Greater))
      .unwrap_or(Ordering::Equal)
  }
}

#[cfg(test)]
mod test {
  use chrono::{TimeZone, Utc};

  use super::*;

  fn get_default_notes() -> Vec<Note> {
    vec![
      Note {
        article_id: "0".into(),
        article_title: "Title 2".into(),
        article_url: "https://example.com/0".into(),
        content: "Content 2".into(),
        tags: vec![],
        created_at: Utc.with_ymd_and_hms(2023, 12, 2, 1, 2, 3).unwrap(),
        updated_at: Utc.with_ymd_and_hms(2023, 12, 2, 1, 2, 3).unwrap(),
      },
      Note {
        article_id: "1".into(),
        article_title: "Title 1".into(),
        article_url: "https://example.com/1".into(),
        content: "Content 1".into(),
        tags: vec!["Tag 1".into(), "Tag 2".into()],
        created_at: Utc.with_ymd_and_hms(2023, 10, 12, 11, 22, 33).unwrap(),
        updated_at: Utc.with_ymd_and_hms(2023, 10, 12, 11, 22, 33).unwrap(),
      },
      Note {
        article_id: "2".into(),
        article_title: "Title 2".into(), // intentionally same as note 0
        article_url: "https://example.com/2".into(),
        content: "Content 3".into(),
        tags: vec![],
        created_at: Utc.with_ymd_and_hms(2024, 1, 2, 1, 2, 3).unwrap(),
        updated_at: Utc.with_ymd_and_hms(2024, 1, 2, 1, 2, 3).unwrap(),
      },
    ]
  }

  fn get_ids(notes: &[Note]) -> Vec<&str> {
    notes.iter().map(|n| n.article_id.as_str()).collect()
  }

  #[test]
  fn sort_single_date() {
    let mut sorter = Sorter::default();
    sorter.set_criteria(vec![SortCriteria::Date]);
    sorter.order = SortOrder::Ascending;

    let mut notes = get_default_notes();
    notes.sort_by(|n1, n2| sorter.sort(n1, n2));
    let ids = get_ids(&notes);
    assert_eq!(ids, vec!["1", "0", "2"], "Date Ascending");

    sorter.order = SortOrder::Descending;
    notes.sort_by(|n1, n2| sorter.sort(n1, n2));
    let ids = get_ids(&notes);
    assert_eq!(ids, vec!["2", "0", "1"], "Date Descending");
  }

  #[test]
  fn sort_single_title() {
    let mut sorter = Sorter::default();
    sorter.set_criteria(vec![SortCriteria::Title]);
    sorter.order = SortOrder::Ascending;

    let mut notes = get_default_notes();
    notes.sort_by(|n1, n2| sorter.sort(n1, n2));
    let ids = get_ids(&notes);
    assert_eq!(ids, vec!["1", "0", "2"], "Title Ascending");

    sorter.order = SortOrder::Descending;
    notes.sort_by(|n1, n2| sorter.sort(n1, n2));
    let ids = get_ids(&notes);
    assert_eq!(ids, vec!["0", "2", "1"], "Title Descending");
  }

  #[test]
  fn sort_multi() {
    let mut sorter = Sorter::default();
    sorter.set_criteria(vec![SortCriteria::Title, SortCriteria::Date]);
    sorter.order = SortOrder::Ascending;

    let mut notes = get_default_notes();
    // Note "3": same title as notes 0 and 2, but date between them
    notes.push(Note {
      article_id: "3".into(),
      article_title: "Title 2".into(),
      article_url: "https://example.com/3".into(),
      content: "Content 4".into(),
      tags: vec![],
      created_at: Utc.with_ymd_and_hms(2023, 11, 15, 1, 2, 3).unwrap(),
      updated_at: Utc.with_ymd_and_hms(2023, 11, 15, 1, 2, 3).unwrap(),
    });

    notes.sort_by(|n1, n2| sorter.sort(n1, n2));
    let ids = get_ids(&notes);
    // Title 1 first, then Title 2 sorted by date ascending: 3 (Nov), 0 (Dec), 2 (Jan)
    assert_eq!(ids, vec!["1", "3", "0", "2"], "Multi Ascending");

    sorter.order = SortOrder::Descending;
    notes.sort_by(|n1, n2| sorter.sort(n1, n2));
    let ids = get_ids(&notes);
    // Title 2 sorted by date descending: 2, 0, 3; then Title 1
    assert_eq!(ids, vec!["2", "0", "3", "1"], "Multi Descending");
  }
}
