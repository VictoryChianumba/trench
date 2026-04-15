use std::collections::VecDeque;

use chrono::{DateTime, Utc};

use crate::Note;

#[derive(Debug)]
/// Keeps history of the changes on notes, enabling undo & redo operations
pub struct HistoryManager {
  undo_stack: VecDeque<Change>,
  redo_stack: VecDeque<Change>,
  /// Sets the size limit of each stack
  stacks_limit: usize,
}

impl HistoryManager {
  pub fn new(stacks_limit: usize) -> Self {
    Self {
      undo_stack: VecDeque::new(),
      redo_stack: VecDeque::new(),
      stacks_limit,
    }
  }

  /// Adds the given history [`Change`] to the corresponding stack of the given [`HistoryStack`]
  /// and keeping the stack within its allowed limit by dropping changes from the bottom if
  /// needed.
  fn add_to_stack(&mut self, change: Change, target: HistoryStack) {
    let stack = match target {
      HistoryStack::Undo => &mut self.undo_stack,
      HistoryStack::Redo => &mut self.redo_stack,
    };
    stack.push_front(change);
    if stack.len() > self.stacks_limit {
      _ = stack.pop_back();
    }
  }

  /// Register Add Change on the corresponding stack of the [`HistoryStack`]
  pub fn register_add(&mut self, target: HistoryStack, note: &Note) {
    log::trace!("History Register Add: Note: {note:?}");
    let change = Change::AddNote { id: note.article_id.clone() };
    self.add_to_stack(change, target);
  }

  /// Register Remove Note Change on the corresponding stack of the [`HistoryStack`]
  pub fn register_remove(&mut self, target: HistoryStack, deleted_note: Note) {
    log::trace!("History Register Remove: Deleted Note: {deleted_note:?}");
    let change = Change::RemoveNote(Box::new(deleted_note));
    self.add_to_stack(change, target);
  }

  /// Register changes on Note attributes on the corresponding stack of the [`HistoryStack`]
  pub fn register_change_attributes(
    &mut self,
    target: HistoryStack,
    note_before_change: &Note,
  ) {
    log::trace!(
      "History Register Change attribute: Note before: {note_before_change:?}"
    );
    let change = Change::NoteAttribute(Box::new(note_before_change.into()));
    self.add_to_stack(change, target);
  }

  /// Register changes on Note content on the corresponding stack of the [`HistoryStack`]
  pub fn register_change_content(
    &mut self,
    target: HistoryStack,
    note_before_change: &Note,
  ) {
    log::trace!(
      "History Register Change content: Note ID: {}",
      note_before_change.article_id
    );
    let change = Change::NoteContent {
      id: note_before_change.article_id.clone(),
      content: note_before_change.content.to_owned(),
    };
    self.add_to_stack(change, target);
  }

  /// Pops the latest undo Change from its stack if available
  pub fn pop_undo(&mut self) -> Option<Change> {
    self.undo_stack.pop_front()
  }

  /// Pops the latest redo Change from its stack if available
  pub fn pop_redo(&mut self) -> Option<Change> {
    self.redo_stack.pop_front()
  }
}

#[derive(Debug, Clone, Copy)]
/// Represents the types of history targets within the [`HistoryManager`]
pub enum HistoryStack {
  Undo,
  Redo,
}

#[derive(Debug, Clone)]
/// Represents a change to the notes and infos about their previous states.
pub enum Change {
  /// Note added with the given id
  AddNote { id: String },
  /// Note removed. It contains the removed note.
  RemoveNote(Box<Note>),
  /// Note attributes changed. It contains the attributes before the change.
  NoteAttribute(Box<NoteAttributes>),
  /// Note content changed. It contains the content before the change.
  NoteContent { id: String, content: String },
}

#[derive(Debug, Clone)]
/// Contains the changes of attributes on a [`Note`] to be saved in the history stacks
pub struct NoteAttributes {
  pub id: String,
  pub created_at: DateTime<Utc>,
  pub article_title: String,
  pub tags: Vec<String>,
}

impl From<&Note> for NoteAttributes {
  fn from(note: &Note) -> Self {
    Self {
      id: note.article_id.clone(),
      created_at: note.created_at,
      article_title: note.article_title.to_owned(),
      tags: note.tags.to_owned(),
    }
  }
}

#[cfg(test)]
mod tests {
  use chrono::{TimeZone, Utc};

  use super::*;

  fn sample_note(id: u32) -> Note {
    Note {
      article_id: id.to_string(),
      article_title: format!("Title {id}"),
      article_url: format!("https://example.com/{id}"),
      content: format!("Content {id}"),
      tags: vec![format!("tag-{id}")],
      created_at: Utc.with_ymd_and_hms(2024, 2, id + 1, 10, 11, 12).unwrap(),
      updated_at: Utc.with_ymd_and_hms(2024, 2, id + 1, 10, 11, 12).unwrap(),
    }
  }

  #[test]
  fn undo_limit_keeps_newest() {
    let mut history = HistoryManager::new(2);

    history.register_add(HistoryStack::Undo, &sample_note(1));
    history.register_add(HistoryStack::Undo, &sample_note(2));
    history.register_add(HistoryStack::Undo, &sample_note(3));

    match history.pop_undo().unwrap() {
      Change::AddNote { id } => assert_eq!(id, "3"),
      change => panic!("unexpected change: {change:?}"),
    }
    match history.pop_undo().unwrap() {
      Change::AddNote { id } => assert_eq!(id, "2"),
      change => panic!("unexpected change: {change:?}"),
    }
    assert!(history.pop_undo().is_none());
  }

  #[test]
  fn redo_stack_is_independent() {
    let mut history = HistoryManager::new(3);

    history.register_add(HistoryStack::Undo, &sample_note(1));
    history.register_add(HistoryStack::Redo, &sample_note(2));

    match history.pop_undo().unwrap() {
      Change::AddNote { id } => assert_eq!(id, "1"),
      change => panic!("unexpected change: {change:?}"),
    }
    match history.pop_redo().unwrap() {
      Change::AddNote { id } => assert_eq!(id, "2"),
      change => panic!("unexpected change: {change:?}"),
    }
  }

  #[test]
  fn attribute_snapshot_is_cloned() {
    let mut history = HistoryManager::new(2);
    let mut note = sample_note(5);

    history.register_change_attributes(HistoryStack::Undo, &note);

    note.article_title = String::from("Changed");
    note.tags.push(String::from("new"));

    match history.pop_undo().unwrap() {
      Change::NoteAttribute(attributes) => {
        assert_eq!(attributes.article_title, "Title 5");
        assert_eq!(attributes.tags, vec![String::from("tag-5")]);
      }
      change => panic!("unexpected change: {change:?}"),
    }
  }

  #[test]
  fn content_snapshot_keeps_previous() {
    let mut history = HistoryManager::new(2);
    let mut note = sample_note(9);

    history.register_change_content(HistoryStack::Redo, &note);
    note.content = String::from("Changed");

    match history.pop_redo().unwrap() {
      Change::NoteContent { id, content } => {
        assert_eq!(id, "9");
        assert_eq!(content, "Content 9");
      }
      change => panic!("unexpected change: {change:?}"),
    }
  }
}
