use doc_model::VisualLineKind;
use ratatui::{
  Frame,
  layout::{Constraint, Direction, Layout, Rect},
  style::{Color, Modifier, Style},
  text::{Line, Span},
  widgets::{Block, Borders, Paragraph},
};

use crate::state::{Mode, Reader, TOC_WIDTH};

// ── Accent palette ────────────────────────────────────────────────────────────

const BABY_BLUE: Color = Color::Rgb(100, 181, 246);
const ACCENT_DIM: Color = Color::Rgb(70, 130, 180);
const MATH_COLOR: Color = Color::Rgb(80, 200, 160);
const TOC_DIM: Color = Color::Rgb(80, 95, 115);
const MONO_COLOR: Color = Color::Rgb(180, 160, 120);
const CODE_BG: Color = Color::Rgb(20, 25, 35);
const CODE_FG: Color = Color::Rgb(160, 200, 180);
const RULE_COLOR: Color = Color::Rgb(55, 65, 80);
const HEADER_BG: Color = Color::Rgb(12, 17, 27);

pub fn draw(frame: &mut Frame, reader: &Reader) {
  let area = frame.area();
  let (header_area, toc_area, content_area, status_area, search_area) =
    split_layout(area, reader);

  if let Some(ha) = header_area {
    draw_header(frame, reader, ha);
  }
  if let Some(ta) = toc_area {
    draw_toc(frame, reader, ta);
  }
  draw_content(frame, reader, content_area);
  draw_status(frame, reader, status_area);
  if reader.mode == Mode::Search {
    draw_search_bar(frame, reader, search_area.unwrap());
  }
}

fn split_layout(
  area: Rect,
  reader: &Reader,
) -> (Option<Rect>, Option<Rect>, Rect, Rect, Option<Rect>) {
  // Optional 1-row header at the very top.
  let (header_area, below_header) = if reader.meta.is_some() {
    let v = Layout::default()
      .direction(Direction::Vertical)
      .constraints([Constraint::Length(1), Constraint::Min(1)])
      .split(area);
    (Some(v[0]), v[1])
  } else {
    (None, area)
  };

  // Optional TOC panel on the left.
  let (toc_area, right) = if reader.toc_visible {
    let h = Layout::default()
      .direction(Direction::Horizontal)
      .constraints([Constraint::Length(TOC_WIDTH as u16), Constraint::Min(1)])
      .split(below_header);
    (Some(h[0]), h[1])
  } else {
    (None, below_header)
  };

  let (content_area, status_area, search_area) = match reader.mode {
    Mode::Normal => {
      let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(right);
      (v[0], v[1], None)
    }
    Mode::Search => {
      let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1), Constraint::Length(1)])
        .split(right);
      (v[0], v[1], Some(v[2]))
    }
  };

  (header_area, toc_area, content_area, status_area, search_area)
}

fn draw_header(frame: &mut Frame, reader: &Reader, area: Rect) {
  let Some(meta) = &reader.meta else { return };
  let w = area.width as usize;
  let title = &meta.title;
  let sep = if meta.authors.is_empty() { "" } else { "  " };
  let raw = format!(" {}{}{}", title, sep, meta.authors);
  let truncated = toc_trunc(&raw, w);
  let header = Paragraph::new(truncated)
    .style(Style::default().bg(HEADER_BG).fg(BABY_BLUE));
  frame.render_widget(header, area);
}

fn draw_content(frame: &mut Frame, reader: &Reader, area: Rect) {
  let ch = area.height as usize;
  let total = reader.total_lines();
  let q = reader.search_query.to_lowercase();

  let lines: Vec<Line> = (0..ch)
    .map(|row| {
      let vl_idx = reader.offset + row;
      if vl_idx >= total {
        return Line::raw("");
      }
      let vl = &reader.visual_lines[vl_idx];
      let is_cursor = row == reader.cursor_y;
      render_visual_line(vl, is_cursor, &q, &reader.search_matches, vl_idx)
    })
    .collect();

  let paragraph = Paragraph::new(lines).block(Block::default());
  frame.render_widget(paragraph, area);
}

fn render_visual_line<'a>(
  vl: &'a doc_model::VisualLine,
  is_cursor: bool,
  query: &str,
  matches: &[usize],
  vl_idx: usize,
) -> Line<'a> {
  let text = &vl.text;
  let bg = if is_cursor { Color::Rgb(30, 40, 55) } else { Color::Reset };

  let base_style = Style::default().bg(bg);

  match &vl.kind {
    VisualLineKind::Blank => Line::styled("", base_style),

    VisualLineKind::Prose => {
      if !query.is_empty() && matches.contains(&vl_idx) {
        highlight_query(text, query, bg)
      } else {
        Line::styled(text.clone(), base_style)
      }
    }

    VisualLineKind::MathLine { .. } => {
      Line::styled(text.clone(), base_style.fg(MATH_COLOR))
    }

    VisualLineKind::Header(level) => {
      let (fg, modifier) = match level {
        1 => (BABY_BLUE, Modifier::BOLD),
        2 => (ACCENT_DIM, Modifier::BOLD),
        _ => (ACCENT_DIM, Modifier::empty()),
      };
      Line::styled(text.clone(), base_style.fg(fg).add_modifier(modifier))
    }

    VisualLineKind::MatrixLine { is_first, is_last } => {
      let prefix = if *is_first { "┌ " } else if *is_last { "└ " } else { "│ " };
      Line::styled(format!("{}{}", prefix, text), base_style.fg(MATH_COLOR))
    }

    VisualLineKind::StyledProse(spans) => {
      if !query.is_empty() && matches.contains(&vl_idx) {
        highlight_spans(spans, query, bg)
      } else {
        let ratatui_spans: Vec<Span> = spans.iter().map(|s| {
          let mut style = base_style;
          if s.bold        { style = style.add_modifier(Modifier::BOLD); }
          if s.italic      { style = style.add_modifier(Modifier::ITALIC); }
          if s.underline   { style = style.add_modifier(Modifier::UNDERLINED); }
          if s.strikethrough { style = style.add_modifier(Modifier::CROSSED_OUT); }
          if s.monospace   { style = style.fg(MONO_COLOR); }
          if let Some((r, g, b)) = s.color { style = style.fg(Color::Rgb(r, g, b)); }
          Span::styled(s.text.clone(), style)
        }).collect();
        Line::from(ratatui_spans)
      }
    }

    VisualLineKind::ListItem { .. } => {
      // text already contains indent + marker prefix from build_visual_lines.
      if !query.is_empty() && matches.contains(&vl_idx) {
        highlight_query(text, query, bg)
      } else {
        Line::styled(text.clone(), base_style)
      }
    }

    VisualLineKind::Code { is_first, is_last } => {
      let prefix = if *is_first { "╔ " } else if *is_last { "╚ " } else { "║ " };
      Line::styled(
        format!("{}{}", prefix, text),
        Style::default().bg(CODE_BG).fg(CODE_FG),
      )
    }

    VisualLineKind::Rule => {
      Line::styled(text.clone(), Style::default().fg(RULE_COLOR))
    }
  }
}

fn highlight_query(text: &str, query: &str, bg: Color) -> Line<'static> {
  let lower = text.to_lowercase();
  let mut spans: Vec<Span<'static>> = Vec::new();
  let mut pos = 0;
  let ql = query.len();

  while let Some(start) = lower[pos..].find(query) {
    let abs = pos + start;
    if abs > pos {
      spans.push(Span::styled(text[pos..abs].to_string(), Style::default().bg(bg)));
    }
    spans.push(Span::styled(
      text[abs..abs + ql].to_string(),
      Style::default().bg(Color::Yellow).fg(Color::Black),
    ));
    pos = abs + ql;
  }
  if pos < text.len() {
    spans.push(Span::styled(text[pos..].to_string(), Style::default().bg(bg)));
  }

  Line::from(spans)
}

/// Render a StyledProse line with search term highlighting.
/// Each span is rendered with its own style; the matching substring is
/// overridden with a yellow-bg highlight wherever it appears.
fn highlight_spans(spans: &[doc_model::InlineSpan], query: &str, bg: Color) -> Line<'static> {
  let mut ratatui_spans: Vec<Span<'static>> = Vec::new();

  for s in spans {
    let mut style = Style::default().bg(bg);
    if s.bold        { style = style.add_modifier(Modifier::BOLD); }
    if s.italic      { style = style.add_modifier(Modifier::ITALIC); }
    if s.underline   { style = style.add_modifier(Modifier::UNDERLINED); }
    if s.strikethrough { style = style.add_modifier(Modifier::CROSSED_OUT); }
    if s.monospace   { style = style.fg(MONO_COLOR); }
    if let Some((r, g, b)) = s.color { style = style.fg(Color::Rgb(r, g, b)); }

    let lower = s.text.to_lowercase();
    let ql = query.len();
    let mut pos = 0;

    while let Some(start) = lower[pos..].find(query) {
      let abs = pos + start;
      if abs > pos {
        ratatui_spans.push(Span::styled(s.text[pos..abs].to_string(), style));
      }
      ratatui_spans.push(Span::styled(
        s.text[abs..abs + ql].to_string(),
        Style::default().bg(Color::Yellow).fg(Color::Black),
      ));
      pos = abs + ql;
    }
    if pos < s.text.len() {
      ratatui_spans.push(Span::styled(s.text[pos..].to_string(), style));
    }
  }

  Line::from(ratatui_spans)
}

fn draw_toc(frame: &mut Frame, reader: &Reader, area: Rect) {
  let panel_h = area.height as usize;
  // 1 char right border + 1 char leading space = 2 chars overhead
  let inner_w = area.width.saturating_sub(2) as usize;
  let cur_sec = reader.current_section_idx();

  // Scroll to keep current section vertically centered in the panel.
  let toc_scroll = cur_sec
    .map(|idx| idx.saturating_sub(panel_h / 2))
    .unwrap_or(0);

  let total = reader.sections.len();

  let lines: Vec<Line> = (0..panel_h)
    .map(|row| {
      let sec_idx = toc_scroll + row;
      if sec_idx >= total {
        return Line::raw("");
      }
      let (_, level, text) = &reader.sections[sec_idx];
      let indent = match level {
        1 => 0usize,
        2 => 2usize,
        _ => 4usize,
      };
      let avail = inner_w.saturating_sub(indent);
      let label = format!(" {}{}", " ".repeat(indent), toc_trunc(text, avail));
      let is_current = cur_sec.map_or(false, |c| c == sec_idx);
      if is_current {
        Line::styled(label, Style::default().fg(BABY_BLUE).add_modifier(Modifier::BOLD))
      } else {
        Line::styled(label, Style::default().fg(TOC_DIM))
      }
    })
    .collect();

  let widget = Paragraph::new(lines).block(
    Block::default()
      .borders(Borders::RIGHT)
      .border_style(Style::default().fg(Color::DarkGray)),
  );
  frame.render_widget(widget, area);
}

fn toc_trunc(s: &str, max: usize) -> String {
  if max == 0 {
    return String::new();
  }
  let count = s.chars().count();
  if count <= max {
    s.to_string()
  } else if max > 1 {
    let end = s.char_indices().nth(max - 1).map(|(i, _)| i).unwrap_or(s.len());
    format!("{}…", &s[..end])
  } else {
    s.chars().take(max).collect()
  }
}

fn draw_status(frame: &mut Frame, reader: &Reader, area: Rect) {
  let cur = reader.current_line() + 1;
  let tot = reader.total_lines();
  let pct = if tot == 0 { 0 } else { cur * 100 / tot };
  let match_info = if !reader.search_matches.is_empty() {
    format!("  [{}/{}]", reader.search_idx + 1, reader.search_matches.len())
  } else {
    String::new()
  };
  let text = format!(" {cur}/{tot}  {pct}%{match_info}");
  let status = Paragraph::new(text)
    .style(Style::default().bg(Color::Rgb(25, 35, 50)).fg(Color::DarkGray));
  frame.render_widget(status, area);
}

fn draw_search_bar(frame: &mut Frame, reader: &Reader, area: Rect) {
  let text = format!("/{}", reader.search_query);
  let bar = Paragraph::new(text)
    .style(Style::default().bg(Color::Rgb(25, 35, 50)).fg(Color::White));
  frame.render_widget(bar, area);
}
