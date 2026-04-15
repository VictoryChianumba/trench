use std::fmt::{Display, Formatter};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, Hash, PartialEq, PartialOrd, Eq)]
pub struct Input {
  pub key_code: KeyCode,
  pub modifiers: KeyModifiers,
}

impl Input {
  pub fn new(key_code: KeyCode, modifiers: KeyModifiers) -> Self {
    Self { key_code, modifiers }
  }
}

impl From<&KeyEvent> for Input {
  fn from(key_event: &KeyEvent) -> Self {
    Self { key_code: key_event.code, modifiers: key_event.modifiers }
  }
}

impl Display for Input {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    let mut char_convert_tmp = [0; 4];
    let key_text = match self.key_code {
      KeyCode::Backspace => "<Backspace>",
      KeyCode::Enter => "Enter",
      KeyCode::Left => "Left",
      KeyCode::Right => "Right",
      KeyCode::Up => "Up",
      KeyCode::Down => "Down",
      KeyCode::Home => "Home",
      KeyCode::End => "End",
      KeyCode::PageUp => "PageUp",
      KeyCode::PageDown => "PageDown",
      KeyCode::Tab => "Tab",
      KeyCode::BackTab => "BackTab",
      KeyCode::Delete => "Delete",
      KeyCode::Insert => "Insert",
      KeyCode::F(_) => "F",
      KeyCode::Char(char) => {
        if char.is_whitespace() {
          "<Space>"
        } else {
          char.encode_utf8(&mut char_convert_tmp)
        }
      }
      KeyCode::Null => "Null",
      KeyCode::Esc => "Esc",
      _ => panic!("{:?} is not implemented", self.key_code),
    };

    if self.modifiers.is_empty() {
      write!(f, "{key_text}")
    } else {
      let mut modifier_text = String::from("<");
      if self.modifiers.contains(KeyModifiers::CONTROL) {
        modifier_text.push_str("Ctrl-");
      }
      if self.modifiers.contains(KeyModifiers::SHIFT) {
        modifier_text.push_str("Shift-");
      }
      if self.modifiers.contains(KeyModifiers::ALT) {
        modifier_text.push_str("Alt-");
      }

      write!(f, "{modifier_text}{key_text}>")
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UICommand {
  Quit,
  ShowHelp,
  CycleFocusedControlForward,
  CycleFocusedControlBack,
  SelectedNextNote,
  SelectedPrevNote,
  CreateNote,
  EditCurrentNote,
  DeleteCurrentNote,
  StartEditNoteContent,
  BackEditorNormalMode,
  SaveNoteContent,
  DiscardChangesNoteContent,
  EnterMultiSelectMode,
  LeaveMultiSelectMode,
  MulSelToggleSelected,
  MulSelSelectAll,
  MulSelSelectNone,
  MulSelInverSelection,
  MulSelDeleteNotes,
  ShowFilter,
  ResetFilter,
  CycleTagFilter,
  ShowFuzzyFind,
  ToggleEditorVisualMode,
  ToggleFullScreenMode,
  CopyOsClipboard,
  CutOsClipboard,
  PasteOsClipboard,
  ShowSortOptions,
  GoToTopNote,
  GoToBottomNote,
  PageUpNotes,
  PageDownNotes,
  Undo,
  Redo,
}

#[derive(Debug, Clone)]
pub struct CommandInfo {
  pub name: String,
  pub description: String,
}

impl CommandInfo {
  pub fn new(name: &str, description: &str) -> Self {
    Self { name: name.to_owned(), description: description.to_owned() }
  }
}

impl UICommand {
  pub fn get_info(&self) -> CommandInfo {
    match self {
      UICommand::Quit => CommandInfo::new("Exit", "Exit the program"),
      UICommand::ShowHelp => {
        CommandInfo::new("Show help", "Show keybindings overview")
      }
      UICommand::CycleFocusedControlForward => CommandInfo::new(
        "Cycle focus forward",
        "Move focus to the next control",
      ),
      UICommand::CycleFocusedControlBack => CommandInfo::new(
        "Cycle focus backward",
        "Move focus to the previous control",
      ),
      UICommand::SelectedNextNote => CommandInfo::new(
        "Select next note",
        "Select next entry in the notes list",
      ),
      UICommand::SelectedPrevNote => CommandInfo::new(
        "Select previous note",
        "Select previous entry in the notes list",
      ),
      UICommand::CreateNote => {
        CommandInfo::new("Create new note", "Opens dialog to add a new note")
      }
      UICommand::EditCurrentNote => CommandInfo::new(
        "Edit current note",
        "Open entry dialog to edit current note if any",
      ),
      UICommand::DeleteCurrentNote => {
        CommandInfo::new("Delete note", "Delete current note if any")
      }
      UICommand::StartEditNoteContent => CommandInfo::new(
        "Edit note content",
        "Start editing current note content in editor",
      ),
      UICommand::BackEditorNormalMode => CommandInfo::new(
        "Back to Editor Normal Mode",
        "Exit editor special modes (insert, visual) and go back to normal mode",
      ),
      UICommand::SaveNoteContent => {
        CommandInfo::new("Save", "Save changes on note content")
      }
      UICommand::DiscardChangesNoteContent => {
        CommandInfo::new("Discard changes", "Discard changes on note content")
      }
      UICommand::EnterMultiSelectMode => CommandInfo::new(
        "Enter notes multi selection mode",
        "Enter multi selection mode for notes when notes list is in focus to work with multiple notes at once",
      ),
      UICommand::LeaveMultiSelectMode => CommandInfo::new(
        "Leave notes multi selection mode",
        "Leave multi selection mode for notes and return to normal mode",
      ),
      UICommand::MulSelToggleSelected => CommandInfo::new(
        "Toggle selected",
        "Toggle if the current note is selected in multi selection mode",
      ),
      UICommand::MulSelSelectAll => CommandInfo::new(
        "Select all notes",
        "Select all notes in multi selection mode",
      ),
      UICommand::MulSelSelectNone => CommandInfo::new(
        "Clear selection",
        "Clear notes selection in multi selection mode",
      ),
      UICommand::MulSelInverSelection => CommandInfo::new(
        "Invert selection",
        "Invert notes selection in multi selection mode",
      ),
      UICommand::MulSelDeleteNotes => CommandInfo::new(
        "Delete selection",
        "Delete selected notes in multi selection mode",
      ),
      UICommand::ShowFilter => {
        CommandInfo::new("Open filter", "Open filter popup for notes")
      }
      UICommand::ResetFilter => {
        CommandInfo::new("Reset filter", "Reset the applied filter on notes")
      }
      UICommand::CycleTagFilter => {
        CommandInfo::new("Cycle Tag Filter", "Cycle through the tag filters")
      }
      UICommand::ShowFuzzyFind => {
        CommandInfo::new("Fuzzy find", "Open fuzzy find popup for notes")
      }
      UICommand::ToggleEditorVisualMode => CommandInfo::new(
        "Toggle Editor Visual Mode",
        "Toggle Editor Visual(Select) Mode when editor is in focus",
      ),
      UICommand::ToggleFullScreenMode => CommandInfo::new(
        "Toggle Full Screen Mode",
        "Maximize the currently selected view",
      ),
      UICommand::CopyOsClipboard => CommandInfo::new(
        "Copy to OS clipboard",
        "Copy selection to operation system clipboard while in editor visual mode",
      ),
      UICommand::CutOsClipboard => CommandInfo::new(
        "Cut to OS clipboard",
        "Cut selection to operation system clipboard while in editor visual mode",
      ),
      UICommand::PasteOsClipboard => CommandInfo::new(
        "Paste OS clipboard content",
        "Paste the operation system clipboard content into the editor",
      ),
      UICommand::ShowSortOptions => CommandInfo::new(
        "Open sort options",
        "Open sort popup to set the sorting options of the notes",
      ),
      UICommand::GoToTopNote => CommandInfo::new(
        "Go to top note",
        "Go to the top entry in the notes list",
      ),
      UICommand::GoToBottomNote => CommandInfo::new(
        "Go to bottom note",
        "Go to the bottom entry in the notes list",
      ),
      UICommand::PageUpNotes => {
        CommandInfo::new("Page Up notes", "Go one page up in the notes list")
      }
      UICommand::PageDownNotes => CommandInfo::new(
        "Page Down notes",
        "Go one page down in the notes list",
      ),
      UICommand::Undo => {
        CommandInfo::new("Undo", "Undo the latest change on notes")
      }
      UICommand::Redo => {
        CommandInfo::new("Redo", "Redo the latest change on notes")
      }
    }
  }
}

#[derive(Debug)]
pub struct Keymap {
  pub key: Input,
  pub command: UICommand,
}

impl Keymap {
  pub fn new(key: Input, command: UICommand) -> Self {
    Self { key, command }
  }
}

pub fn get_global_keymaps() -> Vec<Keymap> {
  vec![
    Keymap::new(
      Input::new(KeyCode::Char('q'), KeyModifiers::NONE),
      UICommand::Quit,
    ),
    // Char '?' isn't recognized on windows
    #[cfg(not(target_os = "windows"))]
    Keymap::new(
      Input::new(KeyCode::Char('?'), KeyModifiers::NONE),
      UICommand::ShowHelp,
    ),
    #[cfg(target_os = "windows")]
    Keymap::new(
      Input::new(KeyCode::Char('h'), KeyModifiers::NONE),
      UICommand::ShowHelp,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('l'), KeyModifiers::CONTROL),
      UICommand::CycleFocusedControlForward,
    ),
    Keymap::new(
      Input::new(KeyCode::Tab, KeyModifiers::NONE),
      UICommand::CycleFocusedControlForward,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('h'), KeyModifiers::CONTROL),
      UICommand::CycleFocusedControlBack,
    ),
    Keymap::new(
      Input::new(KeyCode::BackTab, KeyModifiers::NONE),
      UICommand::CycleFocusedControlBack,
    ),
    Keymap::new(
      Input::new(KeyCode::Enter, KeyModifiers::NONE),
      UICommand::StartEditNoteContent,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('m'), KeyModifiers::CONTROL),
      UICommand::StartEditNoteContent,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('s'), KeyModifiers::NONE),
      UICommand::SaveNoteContent,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('f'), KeyModifiers::CONTROL),
      UICommand::ToggleFullScreenMode,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('t'), KeyModifiers::CONTROL),
      UICommand::CycleTagFilter,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('u'), KeyModifiers::NONE),
      UICommand::Undo,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('U'), KeyModifiers::SHIFT),
      UICommand::Redo,
    ),
  ]
}

pub fn get_entries_list_keymaps() -> Vec<Keymap> {
  vec![
    Keymap::new(
      Input::new(KeyCode::Up, KeyModifiers::NONE),
      UICommand::SelectedPrevNote,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('k'), KeyModifiers::NONE),
      UICommand::SelectedPrevNote,
    ),
    Keymap::new(
      Input::new(KeyCode::Down, KeyModifiers::NONE),
      UICommand::SelectedNextNote,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('j'), KeyModifiers::NONE),
      UICommand::SelectedNextNote,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('n'), KeyModifiers::NONE),
      UICommand::CreateNote,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('e'), KeyModifiers::NONE),
      UICommand::EditCurrentNote,
    ),
    Keymap::new(
      Input::new(KeyCode::Delete, KeyModifiers::NONE),
      UICommand::DeleteCurrentNote,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('d'), KeyModifiers::NONE),
      UICommand::DeleteCurrentNote,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('d'), KeyModifiers::CONTROL),
      UICommand::DeleteCurrentNote,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('v'), KeyModifiers::NONE),
      UICommand::EnterMultiSelectMode,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('f'), KeyModifiers::NONE),
      UICommand::ShowFilter,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('x'), KeyModifiers::NONE),
      UICommand::ResetFilter,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('a'), KeyModifiers::NONE),
      UICommand::ShowFuzzyFind,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('F'), KeyModifiers::SHIFT),
      UICommand::ShowFuzzyFind,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('o'), KeyModifiers::NONE),
      UICommand::ShowSortOptions,
    ),
    Keymap::new(
      Input::new(KeyCode::Home, KeyModifiers::NONE),
      UICommand::GoToTopNote,
    ),
    Keymap::new(
      Input::new(KeyCode::End, KeyModifiers::NONE),
      UICommand::GoToBottomNote,
    ),
    Keymap::new(
      Input::new(KeyCode::PageUp, KeyModifiers::NONE),
      UICommand::PageUpNotes,
    ),
    Keymap::new(
      Input::new(KeyCode::PageDown, KeyModifiers::NONE),
      UICommand::PageDownNotes,
    ),
  ]
}

pub fn get_editor_mode_keymaps() -> Vec<Keymap> {
  vec![
    Keymap::new(
      Input::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
      UICommand::SaveNoteContent,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('q'), KeyModifiers::CONTROL),
      UICommand::DiscardChangesNoteContent,
    ),
    Keymap::new(
      Input::new(KeyCode::Esc, KeyModifiers::NONE),
      UICommand::BackEditorNormalMode,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
      UICommand::BackEditorNormalMode,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('['), KeyModifiers::CONTROL),
      UICommand::BackEditorNormalMode,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('v'), KeyModifiers::NONE),
      UICommand::ToggleEditorVisualMode,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('x'), KeyModifiers::CONTROL),
      UICommand::CutOsClipboard,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('v'), KeyModifiers::CONTROL),
      UICommand::CopyOsClipboard,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
      UICommand::PasteOsClipboard,
    ),
  ]
}

pub fn get_multi_select_keymaps() -> Vec<Keymap> {
  vec![
    Keymap::new(
      Input::new(KeyCode::Char('q'), KeyModifiers::NONE),
      UICommand::LeaveMultiSelectMode,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('v'), KeyModifiers::NONE),
      UICommand::LeaveMultiSelectMode,
    ),
    Keymap::new(
      Input::new(KeyCode::Esc, KeyModifiers::NONE),
      UICommand::LeaveMultiSelectMode,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
      UICommand::LeaveMultiSelectMode,
    ),
    Keymap::new(
      Input::new(KeyCode::Up, KeyModifiers::NONE),
      UICommand::SelectedPrevNote,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('k'), KeyModifiers::NONE),
      UICommand::SelectedPrevNote,
    ),
    Keymap::new(
      Input::new(KeyCode::Down, KeyModifiers::NONE),
      UICommand::SelectedNextNote,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('j'), KeyModifiers::NONE),
      UICommand::SelectedNextNote,
    ),
    Keymap::new(
      Input::new(KeyCode::Char(' '), KeyModifiers::NONE),
      UICommand::MulSelToggleSelected,
    ),
    Keymap::new(
      Input::new(KeyCode::Enter, KeyModifiers::NONE),
      UICommand::MulSelToggleSelected,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('m'), KeyModifiers::CONTROL),
      UICommand::MulSelToggleSelected,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('a'), KeyModifiers::NONE),
      UICommand::MulSelSelectAll,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('x'), KeyModifiers::NONE),
      UICommand::MulSelSelectNone,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('i'), KeyModifiers::NONE),
      UICommand::MulSelInverSelection,
    ),
    Keymap::new(
      Input::new(KeyCode::Char('d'), KeyModifiers::NONE),
      UICommand::MulSelDeleteNotes,
    ),
    Keymap::new(
      Input::new(KeyCode::Delete, KeyModifiers::NONE),
      UICommand::MulSelDeleteNotes,
    ),
    // Char '?' isn't recognized on windows
    #[cfg(not(target_os = "windows"))]
    Keymap::new(
      Input::new(KeyCode::Char('?'), KeyModifiers::NONE),
      UICommand::ShowHelp,
    ),
    #[cfg(target_os = "windows")]
    Keymap::new(
      Input::new(KeyCode::Char('h'), KeyModifiers::NONE),
      UICommand::ShowHelp,
    ),
  ]
}

#[cfg(test)]
mod tests {
  use crossterm::event::KeyEvent;

  use super::*;

  #[test]
  fn display_plain_space() {
    assert_eq!(
      Input::new(KeyCode::Char(' '), KeyModifiers::NONE).to_string(),
      "<Space>"
    );
    assert_eq!(
      Input::new(KeyCode::Left, KeyModifiers::NONE).to_string(),
      "Left"
    );
  }

  #[test]
  fn display_with_modifiers() {
    let input = Input::new(
      KeyCode::Char('x'),
      KeyModifiers::CONTROL | KeyModifiers::SHIFT | KeyModifiers::ALT,
    );

    assert_eq!(input.to_string(), "<Ctrl-Shift-Alt-x>");
  }

  #[test]
  fn from_key_event() {
    let event = KeyEvent::new(KeyCode::PageDown, KeyModifiers::ALT);

    let input = Input::from(&event);

    assert_eq!(input.key_code, KeyCode::PageDown);
    assert_eq!(input.modifiers, KeyModifiers::ALT);
  }

  #[test]
  fn global_bindings_include_undo_redo() {
    let keymaps = get_global_keymaps();

    assert!(keymaps.iter().any(|keymap| {
      keymap.key == Input::new(KeyCode::Char('u'), KeyModifiers::NONE)
        && keymap.command == UICommand::Undo
    }));
    assert!(keymaps.iter().any(|keymap| {
      keymap.key == Input::new(KeyCode::Char('U'), KeyModifiers::SHIFT)
        && keymap.command == UICommand::Redo
    }));
  }
}
