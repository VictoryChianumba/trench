use ratatui::style::{Color, Modifier, Style};

// ---------------------------------------------------------------------------
// ThemeId — identifies a palette variant
// ---------------------------------------------------------------------------

#[derive(
  Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default, Debug,
)]
pub enum ThemeId {
  #[default]
  Dark,
  Light,
  Amoled,
}

impl ThemeId {
  pub fn label(self) -> &'static str {
    match self {
      ThemeId::Dark => "dark",
      ThemeId::Light => "light",
      ThemeId::Amoled => "amoled",
    }
  }

  /// Cycle Dark → Light → Amoled → Dark.
  pub fn cycle(self) -> ThemeId {
    match self {
      ThemeId::Dark => ThemeId::Light,
      ThemeId::Light => ThemeId::Amoled,
      ThemeId::Amoled => ThemeId::Dark,
    }
  }

  pub fn theme(self) -> Theme {
    match self {
      ThemeId::Dark => Theme::dark(),
      ThemeId::Light => Theme::light(),
      ThemeId::Amoled => Theme::amoled(),
    }
  }
}

// ---------------------------------------------------------------------------
// Theme — holds a full color palette
// ---------------------------------------------------------------------------

pub struct Theme {
  pub accent: Color,
  pub header: Color,
  pub text: Color,
  pub text_dim: Color,
  pub border: Color,
  pub border_active: Color,
  pub bg_selection: Color,
  pub bg_code: Color,
  pub bg_chat: Color,
  pub bg_user_msg: Color,
  pub bg_popup: Color,
  pub text_on_accent: Color,
  pub success: Color,
  pub warning: Color,
  pub error: Color,
}

impl Theme {
  /// Slate/blue research-interface design language — the original palette.
  fn dark() -> Self {
    Self {
      accent: Color::Rgb(135, 206, 235),
      header: Color::Rgb(62, 126, 180),
      text: Color::Rgb(218, 224, 230),
      text_dim: Color::Rgb(105, 118, 130),
      border: Color::Rgb(48, 60, 72),
      border_active: Color::Rgb(80, 96, 112),
      bg_selection: Color::Rgb(58, 74, 90),
      bg_code: Color::Rgb(28, 28, 28),
      bg_chat: Color::Rgb(18, 18, 18),
      bg_user_msg: Color::Rgb(35, 35, 35),
      bg_popup: Color::Black,
      text_on_accent: Color::Black,
      success: Color::Rgb(132, 190, 145),
      warning: Color::Rgb(204, 180, 105),
      error: Color::Red,
    }
  }

  /// Light grey background, dark slate text, same accent blue.
  fn light() -> Self {
    Self {
      accent: Color::Rgb(30, 100, 160),
      header: Color::Rgb(20, 70, 130),
      text: Color::Rgb(30, 36, 48),
      text_dim: Color::Rgb(100, 110, 128),
      border: Color::Rgb(180, 190, 200),
      border_active: Color::Rgb(100, 130, 160),
      bg_selection: Color::Rgb(200, 218, 236),
      bg_code: Color::Rgb(240, 242, 244),
      bg_chat: Color::Rgb(248, 249, 250),
      bg_user_msg: Color::Rgb(232, 236, 240),
      bg_popup: Color::Rgb(252, 252, 252),
      text_on_accent: Color::White,
      success: Color::Rgb(40, 140, 70),
      warning: Color::Rgb(160, 120, 20),
      error: Color::Rgb(180, 40, 40),
    }
  }

  /// Pure black backgrounds, bright accents for OLED screens.
  fn amoled() -> Self {
    Self {
      accent: Color::Rgb(0, 200, 255),
      header: Color::Rgb(80, 180, 255),
      text: Color::Rgb(230, 235, 240),
      text_dim: Color::Rgb(130, 140, 150),
      border: Color::Rgb(36, 36, 36),
      border_active: Color::Rgb(70, 70, 70),
      bg_selection: Color::Rgb(20, 40, 60),
      bg_code: Color::Black,
      bg_chat: Color::Black,
      bg_user_msg: Color::Rgb(14, 14, 14),
      bg_popup: Color::Black,
      text_on_accent: Color::Black,
      success: Color::Rgb(0, 220, 120),
      warning: Color::Rgb(255, 190, 0),
      error: Color::Rgb(255, 60, 60),
    }
  }

  // ── Pre-built style helpers ───────────────────────────────────────────────

  pub fn style_default(&self) -> Style {
    Style::default().fg(self.text)
  }

  pub fn style_dim(&self) -> Style {
    Style::default().fg(self.text_dim)
  }

  pub fn style_accent(&self) -> Style {
    Style::default().fg(self.accent)
  }

  pub fn style_header(&self) -> Style {
    Style::default().fg(self.header).add_modifier(Modifier::BOLD)
  }

  pub fn style_border(&self) -> Style {
    Style::default().fg(self.border)
  }

  pub fn style_border_active(&self) -> Style {
    Style::default().fg(self.border_active)
  }

  pub fn style_selection(&self) -> Style {
    Style::default().bg(self.bg_selection).fg(self.text)
  }

  pub fn style_success(&self) -> Style {
    Style::default().fg(self.success)
  }

  pub fn style_warning(&self) -> Style {
    Style::default().fg(self.warning)
  }

  pub fn style_error(&self) -> Style {
    Style::default().fg(self.error)
  }
}

// ---------------------------------------------------------------------------
// Backward-compatible pub const re-exports (Dark palette)
// These keep existing callers that reference `theme::ACCENT` etc. compiling.
// ---------------------------------------------------------------------------

pub const ACCENT: Color = Color::Rgb(135, 206, 235);
pub const HEADER: Color = Color::Rgb(62, 126, 180);
pub const TEXT: Color = Color::Rgb(218, 224, 230);
pub const TEXT_DIM: Color = Color::Rgb(105, 118, 130);
pub const BORDER: Color = Color::Rgb(48, 60, 72);
pub const BORDER_ACTIVE: Color = Color::Rgb(80, 96, 112);
pub const BG_SELECTION: Color = Color::Rgb(58, 74, 90);
pub const BG_CODE: Color = Color::Rgb(28, 28, 28);
pub const BG_CHAT: Color = Color::Rgb(18, 18, 18);
pub const BG_USER_MSG: Color = Color::Rgb(35, 35, 35);
pub const BG_POPUP: Color = Color::Black;
pub const TEXT_ON_ACCENT: Color = Color::Black;
pub const SUCCESS: Color = Color::Rgb(132, 190, 145);
pub const WARNING: Color = Color::Rgb(204, 180, 105);
pub const ERROR: Color = Color::Red;
pub const INFO: Color = ACCENT;

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
