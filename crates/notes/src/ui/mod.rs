pub mod entry_popup;
pub mod export_popup;
pub mod filter_popup;
pub mod fuzz_find;
pub mod help_popup;
pub mod msg_box;
pub mod sort_popup;
pub(crate) mod ui_functions;

/// Generic return type for popups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopupReturn<T> {
    KeepPopup,
    Cancel,
    Apply(T),
}
