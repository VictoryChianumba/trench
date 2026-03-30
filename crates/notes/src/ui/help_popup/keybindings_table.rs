use ratatui::widgets::TableState;

/// A single row in a keybindings help table.
#[derive(Debug, Clone)]
pub struct CommandRow {
    pub key: String,
    pub name: String,
    pub description: String,
}

impl CommandRow {
    pub fn new(
        key: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            name: name.into(),
            description: description.into(),
        }
    }
}

pub trait KeybindingsTable {
    fn get_state_mut(&mut self) -> &mut TableState;
    fn get_rows(&self) -> &[CommandRow];
    fn get_title(&self) -> &str;

    fn select_next(&mut self) {
        let len = self.get_rows().len();
        if len == 0 {
            return;
        }
        let last_index = len - 1;
        let state = self.get_state_mut();
        let new_row = state
            .selected()
            .map(|row| if row >= last_index { 0 } else { row + 1 })
            .unwrap_or(0);
        state.select(Some(new_row));
    }

    fn select_previous(&mut self) {
        let len = self.get_rows().len();
        if len == 0 {
            return;
        }
        let last_index = len - 1;
        let state = self.get_state_mut();
        let new_row = state
            .selected()
            .map(|row| row.checked_sub(1).unwrap_or(last_index))
            .unwrap_or(last_index);
        state.select(Some(new_row));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestBindingsTable {
        state: TableState,
        rows: Vec<CommandRow>,
    }

    impl TestBindingsTable {
        fn new() -> Self {
            Self {
                state: TableState::default(),
                rows: vec![
                    CommandRow::new("q", "Quit", "Quit the application"),
                    CommandRow::new("u", "Undo", "Undo the last action"),
                    CommandRow::new("U", "Redo", "Redo the last undone action"),
                ],
            }
        }
    }

    impl KeybindingsTable for TestBindingsTable {
        fn get_state_mut(&mut self) -> &mut TableState {
            &mut self.state
        }

        fn get_rows(&self) -> &[CommandRow] {
            &self.rows
        }

        fn get_title(&self) -> &str {
            "Test"
        }
    }

    #[test]
    fn next_wraps_to_start() {
        let mut table = TestBindingsTable::new();
        table.state.select(Some(2));

        table.select_next();

        assert_eq!(table.state.selected(), Some(0));
    }

    #[test]
    fn previous_wraps_to_end() {
        let mut table = TestBindingsTable::new();
        table.state.select(Some(0));

        table.select_previous();

        assert_eq!(table.state.selected(), Some(2));
    }

    #[test]
    fn none_selection_picks_edges() {
        let mut table = TestBindingsTable::new();

        table.select_next();
        assert_eq!(table.state.selected(), Some(0));

        table.state.select(None);
        table.select_previous();
        assert_eq!(table.state.selected(), Some(2));
    }
}
