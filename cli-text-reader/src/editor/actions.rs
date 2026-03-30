#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditorAction {
  None,
  NeedsRedraw,
  Quit,
  CommandOutputOpened,
  OpenOverlay,
}

impl EditorAction {
  pub fn requests_redraw(&self) -> bool {
    matches!(
      self,
      Self::NeedsRedraw | Self::CommandOutputOpened | Self::OpenOverlay
    )
  }
}
