/// The format of the math input.
pub enum MathInput<'a> {
  /// Raw LaTeX source, e.g. `\frac{x^2 + 1}{y}`.
  Latex(&'a str),
  /// MathML XML string, e.g. from an EPUB `<math>` block.
  MathMl(&'a str),
}

/// Render math to a Unicode string suitable for terminal display.
///
/// On any rendering error the raw source is returned unchanged so the
/// caller always gets displayable text — never a blank or a panic.
pub fn render(input: MathInput<'_>) -> String {
  match input {
    MathInput::Latex(src) => match tui_math::render_latex(src) {
      Ok(s) => s,
      Err(_) => src.to_string(),
    },
    // tui-math works on MathML internally; expose a direct path for EPUB math.
    // For now fall back to raw — a dedicated MathML path can be added later.
    MathInput::MathMl(src) => src.to_string(),
  }
}
