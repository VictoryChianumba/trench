use ratatui::{
  Frame,
  layout::{Constraint, Direction, Layout, Rect},
  style::{Color, Modifier, Style},
  text::{Line, Span},
  widgets::{Block, Borders, Paragraph},
};

use crate::app::{App, RepoFileKind, RepoPane};
use crate::github::NodeType;
use crate::theme;
use crate::ui::repo_markdown;

pub fn draw_repo_viewer(frame: &mut Frame, app: &mut App) {
  let area = frame.area();

  let rows = Layout::default()
    .direction(Direction::Vertical)
    .constraints([
      Constraint::Length(2), // quiet header + separator
      Constraint::Min(0),    // tree + file
      Constraint::Length(1), // help bar
    ])
    .split(area);

  draw_header(frame, app, rows[0]);
  draw_main(frame, app, rows[1]);
  draw_help(frame, app, rows[2]);
}

// ── Header ───────────────────────────────────────────────────────────────────

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
  let ctx = match &app.repo_context {
    Some(c) => c,
    None => return,
  };

  let rows = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Length(1), Constraint::Length(1)])
    .split(area);

  let repo = format!("github.com/{}/{}", ctx.owner, ctx.repo_name);
  let branch = (!ctx.default_branch.is_empty())
    .then(|| format!("  [{}]", ctx.default_branch))
    .unwrap_or_default();
  let width = rows[0].width as usize;
  let header = Line::from(vec![
    Span::raw(" "),
    Span::styled(
      truncate(&repo, width.saturating_sub(branch.len() + 1)),
      theme::style_accent(),
    ),
    Span::styled(branch, theme::style_dim()),
  ]);
  frame.render_widget(Paragraph::new(header), rows[0]);

  let sep = "─".repeat(area.width as usize);
  frame.render_widget(
    Paragraph::new(Span::styled(sep, theme::style_border())),
    rows[1],
  );
}

// ── Main (tree + file) ───────────────────────────────────────────────────────

fn draw_main(frame: &mut Frame, app: &mut App, area: Rect) {
  let ctx = match app.repo_context.as_mut() {
    Some(c) => c,
    None => return,
  };

  if ctx.no_token {
    let msg = Paragraph::new(vec![
      Line::from(""),
      Line::from(Span::styled(
        "  GitHub token required.",
        Style::default().fg(theme::WARNING).add_modifier(Modifier::BOLD),
      )),
      Line::from(""),
      Line::from(Span::styled(
        "  Set github_token in ~/.config/trench/config.json",
        theme::style_dim(),
      )),
      Line::from(""),
      Line::from(Span::styled(
        "  Example:  { \"github_token\": \"ghp_...\" }",
        theme::style_dim(),
      )),
    ])
    .block(
      Block::default()
        .borders(Borders::ALL)
        .border_style(theme::style_border()),
    );
    frame.render_widget(msg, area);
    return;
  }

  let tree_w = (area.width / 3).max(24).min(48);
  let tree_title = if ctx.tree_path.is_empty() {
    "/".to_string()
  } else {
    format!("/{}/", ctx.tree_path)
  };
  let file_title = ctx
    .file_name
    .as_deref()
    .map(|n| format!(" {n} "))
    .unwrap_or_else(|| " (no file open) ".to_string());
  let (tree_rect, file_rect) = draw_repo_split_box(
    frame,
    area,
    tree_w,
    &tree_title,
    ctx.pane_focus == RepoPane::Tree,
    &file_title,
    ctx.pane_focus == RepoPane::File,
  );

  draw_tree_pane(frame, ctx, tree_rect);
  draw_file_pane(frame, ctx, file_rect);
}

// ── Tree pane ────────────────────────────────────────────────────────────────

fn draw_tree_pane(
  frame: &mut Frame,
  ctx: &crate::app::RepoContext,
  area: Rect,
) {
  // Show status message inline in the tree pane.
  if let Some(ref msg) = ctx.status_message {
    let p = Paragraph::new(Span::styled(
      truncate(msg, area.width as usize),
      Style::default().fg(theme::WARNING),
    ));
    frame.render_widget(p, area);
    return;
  }

  if ctx.tree_nodes.is_empty() {
    let p = Paragraph::new("  (empty)").style(theme::style_dim());
    frame.render_widget(p, area);
    return;
  }

  let visible_h = area.height as usize;
  let scroll = if ctx.tree_cursor >= visible_h {
    ctx.tree_cursor - visible_h + 1
  } else {
    0
  };
  let max_name = area.width.saturating_sub(3) as usize;

  let lines: Vec<Line> = ctx
    .tree_nodes
    .iter()
    .enumerate()
    .skip(scroll)
    .take(visible_h)
    .map(|(i, node)| {
      let icon = match node.node_type {
        NodeType::Dir => "▸ ",
        NodeType::File => "  ",
      };
      let text = format!("{}{}", icon, truncate(&node.name, max_name));

      if i == ctx.tree_cursor {
        Line::from(Span::styled(
          text,
          theme::style_selection().add_modifier(Modifier::BOLD),
        ))
      } else {
        let col = match node.node_type {
          NodeType::Dir => theme::ACCENT,
          NodeType::File => theme::TEXT,
        };
        Line::from(Span::styled(text, Style::default().fg(col)))
      }
    })
    .collect();

  let p = Paragraph::new(lines);
  frame.render_widget(p, area);
}

// ── File pane ────────────────────────────────────────────────────────────────

fn draw_file_pane(
  frame: &mut Frame,
  ctx: &mut crate::app::RepoContext,
  area: Rect,
) {
  if ctx.file_lines.is_empty() {
    let p =
      Paragraph::new("  Navigate the tree and press enter to open a file.")
        .style(theme::style_dim());
    frame.render_widget(p, area);
    return;
  }

  let visible_h = area.height as usize;
  let pane_w = area.width as usize;
  let render_w =
    if ctx.wrap_width > 0 { ctx.wrap_width.min(pane_w) } else { pane_w };
  let h_off = ctx.h_offset;
  let show_pan_indicator = h_off > 0;

  if ctx.file_kind == RepoFileKind::Markdown {
    prepare_markdown_cache(ctx, render_w);
  }

  if ctx.file_kind == RepoFileKind::Markdown {
    let cache = ctx
      .markdown_cache
      .as_ref()
      .expect("markdown cache should be prepared before drawing");
    let lines: Vec<Line> = cache
      .lines
      .iter()
      .skip(ctx.file_scroll)
      .take(visible_h)
      .map(|line| repo_markdown::line_to_ratatui(line, h_off, render_w))
      .collect();
    frame.render_widget(Paragraph::new(lines), area);
  } else if !ctx.file_highlighted.is_empty() {
    // Syntax-highlighted code.
    let total_lines = ctx.file_lines.len();
    let line_num_w = format!("{total_lines}").len();

    let lines: Vec<Line> = ctx
      .file_highlighted
      .iter()
      .enumerate()
      .skip(ctx.file_scroll)
      .take(visible_h)
      .map(|(i, spans)| {
        let mut line_spans = vec![Span::styled(
          format!("{:>line_num_w$} ", i + 1),
          theme::style_dim(),
        )];
        let content: String =
          spans.iter().map(|(_, _, _, t)| t.as_str()).collect();
        let content_sliced = apply_h_offset(&content, h_off, render_w);
        // Re-apply colours by slicing character ranges.
        let mut col_offset = 0usize;
        for (r, g, b, text) in spans {
          let start = col_offset;
          let end = col_offset + text.chars().count();
          col_offset = end;
          let sliced = slice_char_range(
            &content_sliced,
            &content,
            start,
            end,
            h_off,
            render_w,
          );
          if !sliced.is_empty() {
            line_spans.push(Span::styled(
              sliced,
              Style::default().fg(Color::Rgb(*r, *g, *b)),
            ));
          }
        }
        Line::from(line_spans)
      })
      .collect();

    frame.render_widget(Paragraph::new(lines), area);
  } else {
    // Plain text.
    let total_lines = ctx.file_lines.len();
    let line_num_w = format!("{total_lines}").len();

    let lines: Vec<Line> = ctx
      .file_lines
      .iter()
      .enumerate()
      .skip(ctx.file_scroll)
      .take(visible_h)
      .map(|(i, line)| {
        let sliced = apply_h_offset(line, h_off, render_w);
        Line::from(vec![
          Span::styled(format!("{:>line_num_w$} ", i + 1), theme::style_dim()),
          Span::raw(sliced),
        ])
      })
      .collect();

    frame.render_widget(Paragraph::new(lines), area);
  }

  if show_pan_indicator
    && area.width >= 4
    && (ctx.file_kind != RepoFileKind::Markdown
      || ctx.markdown_has_pannable_lines)
  {
    let indicator = format!("◀+{h_off}");
    let x = area.x + area.width.saturating_sub(indicator.len() as u16 + 1);
    let indicator_area =
      Rect { x, y: area.y, width: indicator.len() as u16 + 1, height: 1 };
    let p = Paragraph::new(Span::styled(
      indicator,
      Style::default().fg(theme::WARNING),
    ));
    frame.render_widget(p, indicator_area);
  }
}

fn draw_repo_split_box(
  frame: &mut Frame,
  area: Rect,
  tree_w: u16,
  tree_title: &str,
  tree_focused: bool,
  file_title: &str,
  file_focused: bool,
) -> (Rect, Rect) {
  let border_style = theme::style_border();

  frame.render_widget(
    Block::default().borders(Borders::ALL).border_style(border_style),
    area,
  );

  let inner = Rect {
    x: area.x + 1,
    y: area.y + 1,
    width: area.width.saturating_sub(2),
    height: area.height.saturating_sub(2),
  };

  let tree_w = tree_w.min(inner.width.saturating_sub(2));
  let file_w = inner.width.saturating_sub(tree_w + 1);
  let div_x = inner.x + tree_w;

  if inner.height > 0 {
    let divider: Vec<Line> = (0..inner.height)
      .map(|_| Line::from(Span::styled("│", border_style)))
      .collect();
    frame.render_widget(
      Paragraph::new(divider),
      Rect { x: div_x, y: inner.y, width: 1, height: inner.height },
    );
  }

  frame.render_widget(
    Paragraph::new(Span::styled("┬", border_style)),
    Rect { x: div_x, y: area.y, width: 1, height: 1 },
  );
  if area.height > 1 {
    frame.render_widget(
      Paragraph::new(Span::styled("┴", border_style)),
      Rect { x: div_x, y: area.y + area.height - 1, width: 1, height: 1 },
    );
  }

  draw_split_title(frame, area.x + 1, area.y, tree_w, tree_title, tree_focused);
  draw_split_title(frame, div_x + 1, area.y, file_w, file_title, file_focused);

  let tree_rect =
    Rect { x: inner.x, y: inner.y, width: tree_w, height: inner.height };
  let file_rect =
    Rect { x: div_x + 1, y: inner.y, width: file_w, height: inner.height };

  if tree_focused {
    draw_active_pane_outline(frame, area, tree_rect);
  }
  if file_focused {
    draw_active_pane_outline(frame, area, file_rect);
  }

  (tree_rect, file_rect)
}

fn draw_split_title(
  frame: &mut Frame,
  x: u16,
  y: u16,
  width: u16,
  title: &str,
  focused: bool,
) {
  if width == 0 {
    return;
  }

  let label = format!(" {} ", truncate(title.trim(), width as usize));
  let style = if focused { theme::style_header() } else { theme::style_dim() };
  frame.render_widget(
    Paragraph::new(Span::styled(truncate(&label, width as usize), style)),
    Rect { x, y, width, height: 1 },
  );
}

fn draw_active_pane_outline(
  frame: &mut Frame,
  outer_area: Rect,
  pane_rect: Rect,
) {
  if pane_rect.width == 0 || pane_rect.height == 0 {
    return;
  }

  let outline = Rect {
    x: pane_rect.x.saturating_sub(1),
    y: pane_rect.y.saturating_sub(1),
    width: pane_rect
      .width
      .saturating_add(2)
      .min(outer_area.x + outer_area.width - pane_rect.x.saturating_sub(1)),
    height: pane_rect
      .height
      .saturating_add(2)
      .min(outer_area.y + outer_area.height - pane_rect.y.saturating_sub(1)),
  };

  frame.render_widget(
    Block::default()
      .borders(Borders::ALL)
      .border_style(theme::style_border_active()),
    outline,
  );
}

fn prepare_markdown_cache(ctx: &mut crate::app::RepoContext, render_w: usize) {
  let needs_refresh = ctx
    .markdown_cache
    .as_ref()
    .is_none_or(|cache| cache.wrap_width != render_w);

  if needs_refresh {
    let cache = repo_markdown::render_markdown(&ctx.raw_file_content, render_w);
    ctx.rendered_line_count = cache.lines.len();
    ctx.markdown_has_pannable_lines = cache.has_pannable_lines;
    if !ctx.markdown_has_pannable_lines {
      ctx.h_offset = 0;
    }
    ctx.markdown_cache = Some(cache);
  } else if let Some(cache) = &ctx.markdown_cache {
    ctx.rendered_line_count = cache.lines.len();
    ctx.markdown_has_pannable_lines = cache.has_pannable_lines;
  }

  let max_scroll = ctx.rendered_line_count.saturating_sub(1);
  if ctx.file_scroll > max_scroll {
    ctx.file_scroll = max_scroll;
  }
}

// ── Horizontal offset helpers ────────────────────────────────────────────────

/// Clip a string to [h_off .. h_off + max_w] in character terms.
fn apply_h_offset(s: &str, h_off: usize, max_w: usize) -> String {
  if max_w == 0 {
    return String::new();
  }
  s.chars().skip(h_off).take(max_w).collect()
}

/// Given a syntect span that covers [start, end) in `full`, return the visible
/// portion after applying h_off/render_w. `content_sliced` is precomputed but
/// we re-derive from `full` for correctness.
fn slice_char_range(
  _content_sliced: &str,
  full: &str,
  start: usize,
  end: usize,
  h_off: usize,
  render_w: usize,
) -> String {
  let vis_start = h_off;
  let vis_end = h_off + render_w;
  let s = start.max(vis_start);
  let e = end.min(vis_end);
  if s >= e {
    return String::new();
  }
  full.chars().skip(s).take(e - s).collect()
}

// ── Help bar ─────────────────────────────────────────────────────────────────

fn draw_help(frame: &mut Frame, app: &App, area: Rect) {
  let ctx = match &app.repo_context {
    Some(c) => c,
    None => return,
  };

  let help = if ctx.no_token {
    "q: back".to_string()
  } else {
    match ctx.pane_focus {
      RepoPane::Tree => {
        "j/k: navigate  enter: open  b: up  tab: file  y: copy  q: back"
          .to_string()
      }
      RepoPane::File => {
        let zoom =
          if ctx.file_kind == RepoFileKind::Markdown && ctx.wrap_width > 0 {
            format!("  [zoom:{}]", ctx.wrap_width)
          } else {
            String::new()
          };
        let pan = if ctx.file_kind == RepoFileKind::Markdown
          && !ctx.markdown_has_pannable_lines
        {
          "h/l: pan (code blocks only)"
        } else {
          "h/l: pan"
        };
        let zoom_hint = if ctx.file_kind == RepoFileKind::Markdown {
          "+/-: zoom"
        } else {
          "+/-: zoom (md only)"
        };
        format!(
          "j/k: scroll  {pan}  {zoom_hint}  tab: tree  y: copy  d: dl  q: back{zoom}",
        )
      }
    }
  };

  let p = Paragraph::new(help).style(theme::style_dim());
  frame.render_widget(p, area);
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn truncate(s: &str, max_chars: usize) -> String {
  if max_chars == 0 {
    return String::new();
  }
  let mut chars = s.chars();
  let mut out = String::new();
  let mut count = 0;
  for c in &mut chars {
    if count >= max_chars {
      if chars.next().is_some() {
        out.push('…');
      }
      break;
    }
    out.push(c);
    count += 1;
  }
  out
}
