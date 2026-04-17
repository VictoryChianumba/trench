use ratatui::style::{Color, Modifier, Style};

// ---------------------------------------------------------------------------
// Core palette — slate/blue research-interface design language
// ---------------------------------------------------------------------------

/// Baby blue — primary accent, actionable content, active highlights.
pub const ACCENT: Color = Color::Rgb(135, 206, 235);
/// Darker luminous blue — section/column headers.
pub const HEADER: Color = Color::Rgb(62, 126, 180);
/// Off-white — primary readable text.
pub const TEXT: Color = Color::Rgb(218, 224, 230);
/// Slate mid-tone — secondary/dimmed text.
pub const TEXT_DIM: Color = Color::Rgb(105, 118, 130);
/// Dark slate — borders and separators.
pub const BORDER: Color = Color::Rgb(48, 60, 72);
/// Slightly lighter slate — active/focused border.
pub const BORDER_ACTIVE: Color = Color::Rgb(80, 96, 112);

// ---------------------------------------------------------------------------
// Backgrounds
// ---------------------------------------------------------------------------

/// Selection row background.
pub const BG_SELECTION: Color = Color::Rgb(58, 74, 90);
/// Code block background.
pub const BG_CODE: Color = Color::Rgb(28, 28, 28);
/// Chat panel background.
pub const BG_CHAT: Color = Color::Rgb(18, 18, 18);
/// User message bubble background.
pub const BG_USER_MSG: Color = Color::Rgb(35, 35, 35);
/// Modal/popup background.
pub const BG_POPUP: Color = Color::Black;
/// Text rendered on ACCENT background (e.g. active tab label).
pub const TEXT_ON_ACCENT: Color = Color::Black;

// ---------------------------------------------------------------------------
// Semantic colors
// ---------------------------------------------------------------------------

/// Muted green — success, confirmed, added.
pub const SUCCESS: Color = Color::Rgb(132, 190, 145);
/// Muted amber — warnings, attention, pending.
pub const WARNING: Color = Color::Rgb(204, 180, 105);
/// Red — errors, failures.
pub const ERROR: Color = Color::Red;
/// Informational highlight — alias for ACCENT.
pub const INFO: Color = ACCENT;

// ---------------------------------------------------------------------------
// Pre-built styles
// ---------------------------------------------------------------------------

pub fn style_default() -> Style {
  Style::default().fg(TEXT)
}

pub fn style_dim() -> Style {
  Style::default().fg(TEXT_DIM)
}

pub fn style_accent() -> Style {
  Style::default().fg(ACCENT)
}

pub fn style_header() -> Style {
  Style::default().fg(HEADER).add_modifier(Modifier::BOLD)
}

pub fn style_border() -> Style {
  Style::default().fg(BORDER)
}

pub fn style_border_active() -> Style {
  Style::default().fg(BORDER_ACTIVE)
}

pub fn style_selection() -> Style {
  Style::default().bg(BG_SELECTION).fg(TEXT)
}

pub fn style_success() -> Style {
  Style::default().fg(SUCCESS)
}

pub fn style_warning() -> Style {
  Style::default().fg(WARNING)
}

pub fn style_error() -> Style {
  Style::default().fg(ERROR)
}
