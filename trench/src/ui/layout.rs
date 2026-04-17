use ratatui::{
  Frame,
  layout::{Alignment, Constraint, Direction, Layout, Rect},
  style::{Color, Modifier, Style},
  text::{Line, Span, Text},
  widgets::{
    Block, Borders, Cell, Clear, Paragraph, Row, Scrollbar,
    ScrollbarOrientation, ScrollbarState, Table, Wrap,
  },
};

use super::repo_viewer::draw_repo_viewer;
use crate::app::{
  App, AppView, DiscoverResult, FeedTab, PaneId, SourcesDetectState,
};
use crate::models::{ContentType, SignalLevel, SourcePlatform, WorkflowState};
use crate::theme;

pub const RIGHT_COL_WIDTH: u16 = 36;

const WIDE_TITLE_MIN_WIDTH: u16 = 108;

const TITLE_ART: &[&str] = &[
  " ██████╗ ███╗  ██╗███████╗    ██████╗ ███████╗███████╗███████╗ █████╗ ██████╗  ██████╗██╗  ██╗",
  "██╔═══██╗████╗ ██║██╔════╝    ██╔══██╗██╔════╝██╔════╝██╔════╝██╔══██╗██╔══██╗██╔════╝██║  ██║",
  "██║   ██║██╔██╗██║█████╗      ██████╔╝█████╗  ███████╗█████╗  ███████║██████╔╝██║     ███████║",
  "██║   ██║██║╚████║██╔══╝      ██╔══██╗██╔══╝  ╚════██║██╔══╝  ██╔══██║██╔══██╗██║     ██╔══██║",
  "╚██████╔╝██║ ╚███║███████╗    ██║  ██║███████╗███████║███████╗██║  ██║██║  ██║╚██████╗██║  ██║",
];
const VERSION: &str = "v0.1.x";

pub fn draw(frame: &mut Frame, app: &mut App) {
  let t_total = std::time::Instant::now();
  match app.view {
    AppView::Feed => draw_feed(frame, app),
    AppView::Settings => draw_settings(frame, app),
    AppView::Sources => {
      draw_settings(frame, app);
      draw_sources_popup(frame, app);
    }
    AppView::RepoViewer => draw_repo_viewer(frame, app),
  }
  // Help overlay floats on top of whatever view is rendered.
  if app.help_active {
    draw_help_overlay(frame, app);
  }
  let total_ms = t_total.elapsed().as_millis();
  if total_ms > 8 {
    log::debug!("ui::draw total: {}ms", total_ms);
  }
}

fn draw_feed(frame: &mut Frame, app: &mut App) {
  let area = frame.area();
  let margin = area.width / 20;
  let title_h = title_bar_height(area.width);

  // Fixed zones: title, search=2, footer=2.  Remaining rows split between
  // main panes and (optionally) chat panel.
  let fixed = title_h + 2 + 2;
  let available = area.height.saturating_sub(fixed);

  // Only allocate a dedicated panel row when the chat conversation is open.
  // Session-list and new-session overlays float over the main layout instead.
  let chat_needs_panel =
    app.chat_active && app.chat_ui.as_ref().map_or(true, |c| c.needs_panel());

  let (main_h, chat_h) = if chat_needs_panel {
    let ch = (available / 2).max(15).min(available.saturating_sub(10));
    let mh = available.saturating_sub(ch);
    (mh, ch)
  } else {
    (available, 0)
  };

  // Build row constraints: title | search | [chat?] | main | [chat?] | footer
  // We place chat above or below main depending on `chat_at_top`.
  if chat_needs_panel && app.chat_at_top {
    let rows = Layout::default()
      .direction(Direction::Vertical)
      .constraints([
        Constraint::Length(title_h), // title bar
        Constraint::Length(2),       // search + filter
        Constraint::Length(chat_h),  // chat panel (top)
        Constraint::Length(main_h),  // main panes
        Constraint::Length(2),       // footer
      ])
      .split(area);

    let t = std::time::Instant::now();
    draw_title_bar(frame, rows[0]);
    log::debug!("draw_title_bar: {}ms", t.elapsed().as_millis());

    let t = std::time::Instant::now();
    draw_search_row(frame, app, h_margin(rows[1], margin));
    log::debug!("draw_search_row: {}ms", t.elapsed().as_millis());

    let chat_rect = Some(rows[2]);
    if let Some(chat_ui) = app.chat_ui.as_mut() {
      let t = std::time::Instant::now();
      chat_ui.draw(frame, rows[2]);
      log::debug!("chat_ui.draw (top): {}ms", t.elapsed().as_millis());
    }

    let t = std::time::Instant::now();
    let mr = draw_main_row(frame, app, h_margin(rows[3], margin));
    log::debug!("draw_main_row: {}ms", t.elapsed().as_millis());

    app.update_pane_rects(mr.feed, mr.reader, mr.notes, mr.details, chat_rect);

    let t = std::time::Instant::now();
    draw_footer(frame, app, rows[4]);
    log::debug!("draw_footer: {}ms", t.elapsed().as_millis());
  } else {
    let rows = Layout::default()
      .direction(Direction::Vertical)
      .constraints([
        Constraint::Length(title_h), // title bar
        Constraint::Length(2),       // search + filter
        Constraint::Length(main_h),  // main panes
        Constraint::Length(chat_h), // chat panel (bottom, 0 when inactive or overlay)
        Constraint::Length(2),      // footer
      ])
      .split(area);

    let t = std::time::Instant::now();
    draw_title_bar(frame, rows[0]);
    log::debug!("draw_title_bar: {}ms", t.elapsed().as_millis());

    let t = std::time::Instant::now();
    draw_search_row(frame, app, h_margin(rows[1], margin));
    log::debug!("draw_search_row: {}ms", t.elapsed().as_millis());

    let t = std::time::Instant::now();
    let mr = draw_main_row(frame, app, h_margin(rows[2], margin));
    log::debug!("draw_main_row: {}ms", t.elapsed().as_millis());

    let chat_rect = if chat_needs_panel { Some(rows[3]) } else { None };
    if chat_needs_panel {
      if let Some(chat_ui) = app.chat_ui.as_mut() {
        let t = std::time::Instant::now();
        chat_ui.draw(frame, rows[3]);
        log::debug!("chat_ui.draw (bottom): {}ms", t.elapsed().as_millis());
      }
    }
    app.update_pane_rects(mr.feed, mr.reader, mr.notes, mr.details, chat_rect);

    let t = std::time::Instant::now();
    draw_footer(frame, app, rows[4]);
    log::debug!("draw_footer: {}ms", t.elapsed().as_millis());
  }

  // Session-list / new-session overlay: rendered last so it floats on top.
  if app.chat_active && !chat_needs_panel {
    if let Some(chat_ui) = app.chat_ui.as_mut() {
      chat_ui.draw_overlay(frame, area);
    }
  }
}

// ── Title bar ──────────────────────────────────────────────────────────────

fn title_bar_height(width: u16) -> u16 {
  if width >= WIDE_TITLE_MIN_WIDTH { 6 } else { 2 }
}

fn draw_title_bar(frame: &mut Frame, area: Rect) {
  if area.width < WIDE_TITLE_MIN_WIDTH {
    draw_compact_title_bar(frame, area);
    return;
  }

  // 5 rows for art + 1 row for horizontal separator
  let inner = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Length(5), Constraint::Length(1)])
    .split(area);

  let art_area = inner[0];
  let sep_area = inner[1];

  // ASCII art, centered
  let art_lines: Vec<Line> = TITLE_ART
    .iter()
    .map(|l| {
      Line::from(Span::styled(
        l.to_string(),
        Style::default().fg(theme::ACCENT),
      ))
    })
    .collect();

  let art_para = Paragraph::new(art_lines).alignment(Alignment::Center);
  frame.render_widget(art_para, art_area);

  // Version string: right-aligned without repainting the whole art row.
  let version_w = VERSION.chars().count().min(art_area.width as usize) as u16;
  let version_area = Rect {
    x: art_area.x + art_area.width.saturating_sub(version_w),
    y: art_area.y + art_area.height.saturating_sub(1),
    width: version_w,
    height: 1,
  };
  let version_para = Paragraph::new(VERSION)
    .alignment(Alignment::Right)
    .style(Style::default().fg(theme::TEXT_DIM));
  frame.render_widget(version_para, version_area);

  // Thin separator below art
  let sep_str = "─".repeat(area.width as usize);
  let sep = Paragraph::new(sep_str).style(Style::default().fg(theme::BORDER));
  frame.render_widget(sep, sep_area);
}

fn draw_compact_title_bar(frame: &mut Frame, area: Rect) {
  let inner = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Length(1), Constraint::Length(1)])
    .split(area);

  let title = "One Research";
  let width = inner[0].width as usize;
  let title_line = if width > title.len() + VERSION.len() {
    Line::from(vec![
      Span::styled(
        title,
        Style::default().fg(theme::TEXT).add_modifier(Modifier::BOLD),
      ),
      Span::raw(" ".repeat(width - title.len() - VERSION.len())),
      Span::styled(VERSION, Style::default().fg(theme::TEXT_DIM)),
    ])
  } else {
    Line::from(Span::styled(
      truncate(title, width),
      Style::default().fg(theme::TEXT).add_modifier(Modifier::BOLD),
    ))
  };

  let title_para = Paragraph::new(title_line);
  frame.render_widget(title_para, inner[0]);

  let sep_str = "─".repeat(area.width as usize);
  let sep = Paragraph::new(sep_str).style(Style::default().fg(theme::BORDER));
  frame.render_widget(sep, inner[1]);
}

// ── Search + filter row ────────────────────────────────────────────────────

fn draw_search_row(frame: &mut Frame, app: &App, area: Rect) {
  // Row 0: content; row 1: separator
  let content_area = Rect { height: 1, ..area };
  let sep_area = Rect { y: area.y + 1, height: 1, ..area };

  let cols = Layout::default()
    .direction(Direction::Horizontal)
    .constraints([Constraint::Min(0), Constraint::Length(RIGHT_COL_WIDTH)])
    .split(content_area);

  let search_text = if app.search_active || !app.search_query.is_empty() {
    format!(" / {}", app.search_query)
  } else {
    " / Search items...".to_string()
  };
  let search_style = if app.search_active {
    Style::default().fg(theme::TEXT)
  } else {
    Style::default().fg(theme::TEXT_DIM)
  };
  frame.render_widget(Paragraph::new(search_text).style(search_style), cols[0]);

  let n = app.active_filters.active_count();
  let filter_label =
    if n > 0 { format!(" Filters ({n}) ") } else { " Filters ".to_string() };
  let filter_style = if app.filter_focus {
    Style::default().fg(theme::ACCENT)
  } else {
    Style::default().fg(theme::TEXT_DIM)
  };
  frame
    .render_widget(Paragraph::new(filter_label).style(filter_style), cols[1]);

  let sep = "─".repeat(area.width as usize);
  frame.render_widget(
    Paragraph::new(sep).style(Style::default().fg(theme::BORDER)),
    sep_area,
  );
}

// ── Main row ───────────────────────────────────────────────────────────────

/// Screen rects computed by draw_main_row, passed back to app.update_pane_rects.
struct MainRowRects {
  feed: Option<Rect>,
  reader: Option<Rect>,
  notes: Option<Rect>,
  details: Option<Rect>,
}

fn draw_main_row(frame: &mut Frame, app: &mut App, area: Rect) -> MainRowRects {
  // ── Reader: always full-width or 60/40 split, regardless of terminal width ─
  if app.reader_active && !app.notes_active {
    if let Some(editor) = app.reader.as_mut() {
      let t = std::time::Instant::now();
      editor.update_layout(area);
      cli_text_reader::draw_editor(frame, area, editor);
      log::debug!("draw_editor (full-width): {}ms", t.elapsed().as_millis());
    }
    return MainRowRects {
      feed: None,
      reader: Some(area),
      notes: None,
      details: None,
    };
  }

  if app.reader_active {
    // Reader + notes: outer border, horizontal split regardless of width.
    let inner_w = area.width.saturating_sub(2);
    let notes_w = (inner_w * 40 / 100).max(1);
    let (reader_rect, notes_rect) =
      draw_horiz_split_box(frame, area, notes_w, "Reader", "Notes");
    if let Some(editor) = app.reader.as_mut() {
      let t = std::time::Instant::now();
      editor.update_layout(reader_rect);
      cli_text_reader::draw_editor(frame, reader_rect, editor);
      log::debug!("draw_editor (split): {}ms", t.elapsed().as_millis());
    }
    if let Some(notes_app) = app.notes_app.as_mut() {
      let t = std::time::Instant::now();
      notes::draw(frame, notes_rect, notes_app);
      log::debug!("notes::draw: {}ms", t.elapsed().as_millis());
    }
    return MainRowRects {
      feed: None,
      reader: Some(reader_rect),
      notes: Some(notes_rect),
      details: None,
    };
  }

  // ── Narrow mode (< 100 cols): vertical stack — feed top, details/notes bottom ──
  if area.width < 100 {
    let bottom_title = if app.notes_active {
      "Notes"
    } else if app.filter_focus {
      "Filters"
    } else {
      "Details"
    };
    let (feed_rect, bottom_rect) =
      draw_vert_split_box(frame, area, "Feed", bottom_title);

    let t = std::time::Instant::now();
    draw_feed_pane(frame, app, feed_rect);
    log::debug!("draw_item_table (narrow): {}ms", t.elapsed().as_millis());

    let mut details_rect: Option<Rect> = None;
    if app.notes_active {
      if let Some(notes_app) = app.notes_app.as_mut() {
        let t = std::time::Instant::now();
        notes::draw(frame, bottom_rect, notes_app);
        log::debug!("notes::draw (narrow): {}ms", t.elapsed().as_millis());
      }
    } else if app.filter_focus {
      let t = std::time::Instant::now();
      draw_filter_panel(frame, app, bottom_rect);
      log::debug!("draw_filter_panel (narrow): {}ms", t.elapsed().as_millis());
    } else {
      details_rect = Some(bottom_rect);
      let t = std::time::Instant::now();
      draw_details_panel(frame, app, bottom_rect);
      log::debug!("draw_details_panel (narrow): {}ms", t.elapsed().as_millis());
    }

    return MainRowRects {
      feed: Some(feed_rect),
      reader: None,
      notes: if app.notes_active { Some(bottom_rect) } else { None },
      details: details_rect,
    };
  }

  // ── Wide mode (>= 100 cols): single outer border, feed left, right panel ──
  let inner_w = area.width.saturating_sub(2);
  let right_w = if app.notes_active {
    (inner_w * 40 / 100).max(1)
  } else {
    RIGHT_COL_WIDTH.min(inner_w.saturating_sub(2))
  };
  let right_title = if app.notes_active {
    "Notes"
  } else if app.filter_focus {
    "Filters"
  } else {
    "Details"
  };

  let (feed_rect, right_rect) =
    draw_horiz_split_box(frame, area, right_w, "Feed", right_title);

  let t = std::time::Instant::now();
  draw_feed_pane(frame, app, feed_rect);
  log::debug!("draw_item_table: {}ms", t.elapsed().as_millis());

  let mut details_rect: Option<Rect> = None;
  if app.notes_active {
    if let Some(notes_app) = app.notes_app.as_mut() {
      let t = std::time::Instant::now();
      notes::draw(frame, right_rect, notes_app);
      log::debug!("notes::draw: {}ms", t.elapsed().as_millis());
    }
  } else if app.filter_focus {
    let t = std::time::Instant::now();
    draw_filter_panel(frame, app, right_rect);
    log::debug!("draw_filter_panel: {}ms", t.elapsed().as_millis());
  } else {
    details_rect = Some(right_rect);
    let t = std::time::Instant::now();
    draw_details_panel(frame, app, right_rect);
    log::debug!("draw_details_panel: {}ms", t.elapsed().as_millis());
  }

  MainRowRects {
    feed: Some(feed_rect),
    reader: None,
    notes: if app.notes_active { Some(right_rect) } else { None },
    details: details_rect,
  }
}

fn draw_feed_pane(frame: &mut Frame, app: &mut App, area: Rect) {
  if area.height == 0 {
    return;
  }
  let rows = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Length(1), Constraint::Min(0)])
    .split(area);
  draw_feed_tab_bar(frame, app, rows[0]);
  draw_item_table(frame, app, rows[1]);
}

fn draw_feed_tab_bar(frame: &mut Frame, app: &App, area: Rect) {
  const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
  let inbox_style = if app.feed_tab == FeedTab::Inbox {
    Style::default().fg(theme::TEXT).add_modifier(Modifier::BOLD)
  } else {
    Style::default().fg(theme::TEXT_DIM)
  };
  let discovery_style = if app.feed_tab == FeedTab::Discoveries {
    Style::default().fg(theme::TEXT).add_modifier(Modifier::BOLD)
  } else {
    Style::default().fg(theme::TEXT_DIM)
  };
  let spin = if app.discovery_loading {
    format!(" {}", SPINNER[app.spinner_frame % SPINNER.len()])
  } else {
    String::new()
  };
  let line = Line::from(vec![
    Span::styled(format!("[Inbox {}]", app.items.len()), inbox_style),
    Span::raw("  "),
    Span::styled(
      format!("[Discoveries {}{}]", app.discovery_items.len(), spin),
      discovery_style,
    ),
  ]);
  frame.render_widget(Paragraph::new(line), area);
}

fn draw_item_table(frame: &mut Frame, app: &mut App, area: Rect) {
  let t_item_table = std::time::Instant::now();

  let header = Row::new(vec![
    Cell::from(" ").style(Style::default().fg(theme::TEXT_DIM)),
    Cell::from("Source")
      .style(Style::default().fg(theme::HEADER).add_modifier(Modifier::BOLD)),
    Cell::from("Type")
      .style(Style::default().fg(theme::HEADER).add_modifier(Modifier::BOLD)),
    Cell::from("Title")
      .style(Style::default().fg(theme::HEADER).add_modifier(Modifier::BOLD)),
    Cell::from("Author")
      .style(Style::default().fg(theme::HEADER).add_modifier(Modifier::BOLD)),
    Cell::from("Date")
      .style(Style::default().fg(theme::HEADER).add_modifier(Modifier::BOLD)),
    Cell::from("State")
      .style(Style::default().fg(theme::HEADER).add_modifier(Modifier::BOLD)),
  ])
  .style(Style::default().add_modifier(Modifier::UNDERLINED))
  .height(1);

  // Inner area: scrollbar and content stay inside the border.
  let inner = area;

  // Available width for title column: total inner width minus fixed cols.
  // sig(1) + source(5) + type(6) + author(14) + date(10) + state(8) + separators(6)
  let title_col_w =
    (inner.width.saturating_sub(1 + 5 + 6 + 14 + 10 + 8 + 6)) as usize;
  let title_wrap_w = title_col_w.max(10);

  // Viewport height in rows (inner height minus 1 header row).
  let viewport_rows = inner.height.saturating_sub(1) as usize;

  // ── Auto scroll tracking — item-count-based ───────────────────────────────
  // Count and visible_count computed in a scoped borrow so list_offset can be
  // mutated afterwards without a live reference into app.items.
  let (total_items_pre, visible_count) = {
    let v = app.visible_items();
    let total = v.len();
    let vc = count_visible_items(
      &v,
      app.active_list_offset(),
      viewport_rows,
      title_wrap_w,
    );
    (total, vc)
  };

  let mut list_offset = app.active_list_offset();
  let selected_index = app.active_selected_index();

  if selected_index < list_offset {
    // Selection moved above the window — scroll up.
    list_offset = selected_index;
  } else if visible_count >= 2
    && selected_index >= list_offset + visible_count.saturating_sub(2)
  {
    // Selection is within 2 items of the bottom edge — scroll down.
    list_offset = (selected_index + 2).saturating_sub(visible_count);
  }
  list_offset = list_offset.min(total_items_pre.saturating_sub(1));
  app.set_active_list_offset(list_offset);

  // Now get the full visible slice for rendering.
  let visible = app.visible_items();
  let total_items = visible.len();

  // ── Slice to visible window — trust app.list_offset as first visible item ─
  // Take viewport_rows + 2 extra so the last row is never clipped even when
  // an item spans 2 rows.
  let start = app.active_list_offset().min(total_items.saturating_sub(1));
  let end = (start + viewport_rows + 2).min(total_items);
  let window = &visible[start..end];

  // ── Single textwrap pass over visible window only ─────────────────────────
  // Produces (row_height, title_lines) together — no second wrap call needed.
  let t_heights = std::time::Instant::now();
  let window_data: Vec<(u16, Vec<Line>)> = window
    .iter()
    .map(|item| {
      let mut raw_lines = textwrap::wrap(&item.title, title_wrap_w);
      let row_height = raw_lines.len().min(2).max(1) as u16;
      if raw_lines.len() > 2 {
        raw_lines.truncate(2);
        if let Some(last) = raw_lines.last_mut() {
          let s = last.clone().into_owned();
          let max_chars = title_wrap_w.saturating_sub(1);
          let trimmed = safe_truncate_chars(&s, max_chars);
          *last = std::borrow::Cow::Owned(format!("{trimmed}…"));
        }
      }
      let title_lines: Vec<Line> =
        raw_lines.into_iter().map(|l| Line::from(l.into_owned())).collect();
      (row_height, title_lines)
    })
    .collect();
  log::debug!(
    "window textwrap ({} items): {}ms",
    window.len(),
    t_heights.elapsed().as_millis()
  );

  // ── Build rows for visible window only ────────────────────────────────────
  let t_rows = std::time::Instant::now();
  let rows: Vec<Row> = window
    .iter()
    .enumerate()
    .map(|(i, item)| {
      let item_idx = start + i;
      let is_selected = item_idx == app.active_selected_index();
      let (row_height, title_lines) = &window_data[i];

      let signal_style = match item.signal {
        crate::models::SignalLevel::Primary => {
          Style::default().fg(theme::ACCENT)
        }
        crate::models::SignalLevel::Secondary => {
          Style::default().fg(theme::TEXT_DIM)
        }
        crate::models::SignalLevel::Tertiary => {
          Style::default().fg(theme::BORDER)
        }
      };

      let row_style = if is_selected {
        Style::default()
          .bg(theme::BG_SELECTION)
          .fg(theme::TEXT)
          .add_modifier(Modifier::BOLD)
      } else {
        Style::default()
      };

      let author =
        truncate(item.authors.first().map(|s| s.as_str()).unwrap_or(""), 13);

      Row::new(vec![
        Cell::from(item.signal.indicator()).style(signal_style),
        Cell::from(if item.source_name.is_empty() {
          item.source_platform.short_label().to_string()
        } else {
          item.source_name.clone()
        })
        .style(Style::default().fg(theme::ACCENT)),
        Cell::from(item.content_type.short_label())
          .style(Style::default().fg(theme::TEXT_DIM)),
        Cell::from(Text::from(title_lines.clone())),
        Cell::from(author).style(Style::default().fg(theme::TEXT_DIM)),
        Cell::from(item.published_at.as_str())
          .style(Style::default().fg(theme::TEXT_DIM)),
        Cell::from(item.workflow_state.short_label())
          .style(Style::default().fg(theme::TEXT_DIM)),
      ])
      .style(row_style)
      .height(*row_height)
    })
    .collect();
  log::debug!(
    "rows build ({} window items): {}ms",
    window.len(),
    t_rows.elapsed().as_millis()
  );

  let table = Table::new(
    rows,
    [
      Constraint::Length(1),
      Constraint::Length(5),
      Constraint::Length(6),
      Constraint::Min(0),
      Constraint::Length(14),
      Constraint::Length(10),
      Constraint::Length(8),
    ],
  )
  .header(header)
  .row_highlight_style(Style::default());

  let t_render = std::time::Instant::now();
  frame.render_widget(table, area);
  log::debug!(
    "frame.render_widget(table): {}ms",
    t_render.elapsed().as_millis()
  );

  // Scrollbar uses item indices for proportions — no full-list row count needed.
  if total_items > 0 {
    let mut scrollbar_state = ScrollbarState::new(total_items)
      .position(start)
      .viewport_content_length(viewport_rows);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
      .begin_symbol(None)
      .end_symbol(None);
    frame.render_stateful_widget(scrollbar, inner, &mut scrollbar_state);
  }
  log::debug!(
    "draw_item_table total: {}ms ({} total items, {} in window)",
    t_item_table.elapsed().as_millis(),
    total_items,
    window.len()
  );
}

fn draw_filter_panel(frame: &mut Frame, app: &App, area: Rect) {
  let inner = area;
  let focused = app.filter_focus;

  let f = &app.active_filters;
  let c = app.filter_cursor;
  let mut s: usize = 0;
  let mut lines: Vec<Line> = Vec::new();
  let mut cursor_line: usize = 0;

  let hrule = "\u{2500}".repeat(inner.width as usize);

  lines.push(filter_header("Source"));
  for name in app.filter_source_names() {
    let active = f.sources.contains(&name);
    let cursor = focused && s == c;
    if cursor {
      cursor_line = lines.len();
    }
    let checkbox = if active { "[x]" } else { "[ ]" };
    let line = if cursor {
      let hl = Style::default()
        .bg(theme::BG_SELECTION)
        .fg(theme::TEXT)
        .add_modifier(Modifier::BOLD);
      Line::from(vec![
        Span::styled("  ", hl),
        Span::styled(checkbox, hl),
        Span::styled(" ", hl),
        Span::styled(name, hl),
      ])
    } else if active {
      Line::from(vec![
        Span::raw("  "),
        Span::styled(checkbox, Style::default().fg(theme::ACCENT)),
        Span::raw(" "),
        Span::styled(name, Style::default().fg(theme::ACCENT)),
      ])
    } else {
      Line::from(vec![
        Span::raw("  "),
        Span::styled(checkbox, Style::default().fg(theme::TEXT_DIM)),
        Span::raw(" "),
        Span::raw(name),
      ])
    };
    lines.push(line);
    s += 1;
  }
  lines.push(Line::from(""));

  lines.push(filter_header("Signal"));
  if focused && s == c {
    cursor_line = lines.len();
  }
  lines.push(filter_row(
    "primary",
    f.signals.contains(&SignalLevel::Primary),
    focused && s == c,
  ));
  s += 1;
  if focused && s == c {
    cursor_line = lines.len();
  }
  lines.push(filter_row(
    "secondary",
    f.signals.contains(&SignalLevel::Secondary),
    focused && s == c,
  ));
  s += 1;
  if focused && s == c {
    cursor_line = lines.len();
  }
  lines.push(filter_row(
    "tertiary",
    f.signals.contains(&SignalLevel::Tertiary),
    focused && s == c,
  ));
  s += 1;
  lines.push(Line::from(""));

  lines.push(filter_header("Type"));
  if focused && s == c {
    cursor_line = lines.len();
  }
  lines.push(filter_row(
    "paper",
    f.content_types.contains(&ContentType::Paper),
    focused && s == c,
  ));
  s += 1;
  if focused && s == c {
    cursor_line = lines.len();
  }
  lines.push(filter_row(
    "article",
    f.content_types.contains(&ContentType::Article),
    focused && s == c,
  ));
  s += 1;
  if focused && s == c {
    cursor_line = lines.len();
  }
  lines.push(filter_row(
    "digest",
    f.content_types.contains(&ContentType::Digest),
    focused && s == c,
  ));
  s += 1;
  lines.push(Line::from(""));

  lines.push(filter_header("State"));
  if focused && s == c {
    cursor_line = lines.len();
  }
  lines.push(filter_row(
    "inbox",
    f.workflow_states.contains(&WorkflowState::Inbox),
    focused && s == c,
  ));
  s += 1;
  if focused && s == c {
    cursor_line = lines.len();
  }
  lines.push(filter_row(
    "skimmed",
    f.workflow_states.contains(&WorkflowState::Skimmed),
    focused && s == c,
  ));
  s += 1;
  if focused && s == c {
    cursor_line = lines.len();
  }
  lines.push(filter_row(
    "queued",
    f.workflow_states.contains(&WorkflowState::Queued),
    focused && s == c,
  ));
  s += 1;
  if focused && s == c {
    cursor_line = lines.len();
  }
  lines.push(filter_row(
    "read",
    f.workflow_states.contains(&WorkflowState::DeepRead),
    focused && s == c,
  ));
  s += 1;
  if focused && s == c {
    cursor_line = lines.len();
  }
  lines.push(filter_row(
    "archived",
    f.workflow_states.contains(&WorkflowState::Archived),
    focused && s == c,
  ));
  s += 1;

  lines
    .push(Line::from(Span::styled(hrule, Style::default().fg(theme::BORDER))));

  let clear_hl = focused && s == c;
  if clear_hl {
    cursor_line = lines.len();
  }
  let clear_style = if clear_hl {
    Style::default()
      .bg(theme::BG_SELECTION)
      .fg(theme::TEXT)
      .add_modifier(Modifier::BOLD)
  } else {
    Style::default().fg(theme::TEXT_DIM)
  };
  lines
    .push(Line::from(Span::styled("[c] clear all".to_string(), clear_style)));

  let total_lines = lines.len();
  let visible_height = inner.height as usize;

  let scroll_offset = if cursor_line < visible_height {
    0
  } else {
    cursor_line.saturating_sub(visible_height.saturating_sub(2))
  };

  let para = Paragraph::new(lines).scroll((scroll_offset as u16, 0));
  frame.render_widget(para, inner);

  if total_lines > visible_height {
    let mut sb_state = ScrollbarState::new(total_lines)
      .position(scroll_offset)
      .viewport_content_length(visible_height);
    let sb = Scrollbar::new(ScrollbarOrientation::VerticalRight)
      .begin_symbol(None)
      .end_symbol(None);
    frame.render_stateful_widget(sb, inner, &mut sb_state);
  }
}

fn draw_details_panel(frame: &mut Frame, app: &mut App, area: Rect) {
  let t_details = std::time::Instant::now();

  // Add 1-char left margin so text doesn't abut the divider.
  let inner =
    Rect { x: area.x + 1, width: area.width.saturating_sub(1), ..area };

  // Reset scroll when the selected item changes, before borrowing item data.
  {
    let current_url = app.selected_item().map(|i| i.url.clone());
    if current_url != app.details_last_item_url {
      app.details_scroll = 0;
      app.details_last_item_url = current_url;
    }
  }

  if let Some(item) = app.selected_item() {
    let tags = item.domain_tags.join(", ");
    let authors = item.authors.join(", ");

    let mut lines: Vec<Line> = vec![
      Line::from(Span::styled(
        truncate(&item.title, inner.width as usize),
        Style::default().fg(theme::TEXT).add_modifier(Modifier::BOLD),
      )),
      Line::from(""),
      Line::from(vec![
        Span::styled("Authors  ", Style::default().fg(theme::TEXT_DIM)),
        Span::raw(truncate(&authors, (inner.width as usize).saturating_sub(9))),
      ]),
      Line::from(vec![
        Span::styled("Source   ", Style::default().fg(theme::TEXT_DIM)),
        Span::styled(
          if item.source_name.is_empty() {
            item.source_platform.short_label().to_string()
          } else {
            item.source_name.clone()
          },
          Style::default().fg(theme::ACCENT),
        ),
        Span::raw("  "),
        Span::styled(
          item.content_type.short_label(),
          Style::default().fg(theme::TEXT_DIM),
        ),
      ]),
    ];

    if item.source_platform == SourcePlatform::HuggingFace {
      lines.push(Line::from(vec![
        Span::styled("Upvotes  ", Style::default().fg(theme::TEXT_DIM)),
        Span::styled(
          format!("\u{2191} {}", item.upvote_count),
          Style::default().fg(theme::SUCCESS),
        ),
      ]));
    }

    if let Some(ref repo) = item.github_repo {
      let display = repo.strip_prefix("https://").unwrap_or(repo.as_str());
      lines.push(Line::from(vec![
        Span::styled("Repo     ", Style::default().fg(theme::TEXT_DIM)),
        Span::styled(
          truncate(display, (inner.width as usize).saturating_sub(9)),
          Style::default().fg(theme::ACCENT),
        ),
      ]));
    }

    lines.extend([
      Line::from(vec![
        Span::styled("Tags     ", Style::default().fg(theme::TEXT_DIM)),
        Span::raw(truncate(&tags, (inner.width as usize).saturating_sub(9))),
      ]),
      Line::from(vec![
        Span::styled("State    ", Style::default().fg(theme::TEXT_DIM)),
        Span::raw(item.workflow_state.short_label()),
      ]),
      Line::from(vec![
        Span::styled("Date     ", Style::default().fg(theme::TEXT_DIM)),
        Span::raw(item.published_at.as_str()),
      ]),
    ]);

    if !item.benchmark_results.is_empty() {
      lines.push(Line::from(""));
      lines.push(Line::from(Span::styled(
        "Benchmarks",
        Style::default().fg(theme::HEADER).add_modifier(Modifier::UNDERLINED),
      )));
      for b in item.benchmark_results.iter().take(3) {
        lines.push(Line::from(Span::styled(
          truncate(
            &format!("  {}/{}: {} ({})", b.task, b.dataset, b.score, b.metric),
            inner.width as usize,
          ),
          Style::default().fg(theme::TEXT_DIM),
        )));
      }
    }

    lines.extend([
      Line::from(""),
      Line::from(Span::styled(
        "Summary",
        Style::default().fg(theme::HEADER).add_modifier(Modifier::UNDERLINED),
      )),
      Line::from(item.summary_short.as_str()),
      Line::from(""),
      Line::from(Span::styled(
        "URL",
        Style::default().fg(theme::HEADER).add_modifier(Modifier::UNDERLINED),
      )),
      Line::from(Span::styled(
        truncate(&item.url, inner.width as usize),
        Style::default().fg(theme::TEXT_DIM),
      )),
    ]);

    if let Some(notif) = &app.notification {
      if app.notification_item_id.as_deref() == Some(item.url.as_str()) {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
          notif.as_str(),
          Style::default().fg(theme::WARNING),
        )));
      }
    }

    if item.github_owner.is_some() && item.github_repo_name.is_some() {
      lines.push(Line::from(""));
      lines.push(Line::from(Span::styled(
        "Repo linked: press v to view",
        Style::default().fg(theme::WARNING),
      )));
    }

    let visible_height = inner.height as usize;
    let inner_width = inner.width.max(1) as usize;

    // Estimate physical rendered line count by accounting for text wrapping.
    // Each logical line with char_count characters takes ceil(char_count / inner_width)
    // physical rows (minimum 1). This matches what Paragraph { Wrap } actually renders.
    let physical_lines: usize = lines
      .iter()
      .map(|line| {
        let char_count: usize =
          line.spans.iter().map(|s| s.content.chars().count()).sum();
        ((char_count + inner_width - 1) / inner_width).max(1)
      })
      .sum();

    // Clamp at render time — don't write back while item is borrowed.
    let max_scroll = physical_lines.saturating_sub(visible_height);
    let scroll = app.details_scroll.min(max_scroll);

    let para = Paragraph::new(lines)
      .wrap(Wrap { trim: false })
      .scroll((scroll as u16, 0));
    let t_para = std::time::Instant::now();
    frame.render_widget(para, inner);
    log::debug!(
      "details Paragraph render ({} physical lines): {}ms",
      physical_lines,
      t_para.elapsed().as_millis()
    );

    if physical_lines > visible_height {
      let mut sb_state = ScrollbarState::new(physical_lines)
        .position(scroll)
        .viewport_content_length(visible_height);
      let sb = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None);
      frame.render_stateful_widget(sb, inner, &mut sb_state);
    }
  } else {
    let empty = Paragraph::new("No item selected")
      .style(Style::default().fg(theme::TEXT_DIM));
    frame.render_widget(empty, inner);
  }
  log::debug!(
    "draw_details_panel total: {}ms",
    t_details.elapsed().as_millis()
  );
}

fn filter_header(label: &'static str) -> Line<'static> {
  Line::from(Span::styled(
    label,
    Style::default().fg(theme::HEADER).add_modifier(Modifier::BOLD),
  ))
}

fn filter_row(
  label: &'static str,
  active: bool,
  cursor: bool,
) -> Line<'static> {
  let checkbox = if active { "[x]" } else { "[ ]" };
  if cursor {
    let hl = Style::default()
      .bg(theme::BG_SELECTION)
      .fg(theme::TEXT)
      .add_modifier(Modifier::BOLD);
    Line::from(vec![
      Span::styled("  ", hl),
      Span::styled(checkbox, hl),
      Span::styled(" ", hl),
      Span::styled(label, hl),
    ])
  } else if active {
    Line::from(vec![
      Span::raw("  "),
      Span::styled(checkbox, Style::default().fg(theme::ACCENT)),
      Span::raw(" "),
      Span::styled(label, Style::default().fg(theme::ACCENT)),
    ])
  } else {
    Line::from(vec![
      Span::raw("  "),
      Span::styled(checkbox, Style::default().fg(theme::TEXT_DIM)),
      Span::raw(" "),
      Span::raw(label),
    ])
  }
}

// ── Footer ─────────────────────────────────────────────────────────────────

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
  let rows = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Length(1), Constraint::Length(1)])
    .split(area);

  // Row 0: status / loading indicator
  let repo_hint: Option<(&'static str, Color)> = (!app.reader_active
    && !app.chat_fullscreen
    && app.focused_pane == PaneId::Feed)
    .then(|| app.selected_item())
    .flatten()
    .and_then(|item| {
      if item.github_owner.is_some() && item.github_repo_name.is_some() {
        Some(("  │  v: repo viewer", theme::SUCCESS))
      } else {
        None
      }
    });

  let stats_line: Line = if app.fulltext_loading {
    const SPINNER: &[&str] =
      &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let spin = SPINNER[app.spinner_frame % SPINNER.len()];
    Line::from(Span::styled(
      format!("{spin} fetching article…"),
      Style::default().fg(theme::WARNING),
    ))
  } else if let Some(msg) = &app.status_message {
    Line::from(Span::styled(msg.as_str(), Style::default().fg(theme::WARNING)))
  } else if app.is_loading {
    const SPINNER: &[&str] =
      &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let spin = SPINNER[app.spinner_frame % SPINNER.len()];
    let sources = app.loading_sources.join(", ");
    let prefix = if app.is_refreshing { "↻ refreshing" } else { "fetching" };
    Line::from(Span::styled(
      format!("{spin} {prefix}: {}  │  {} items", sources, app.items.len()),
      Style::default().fg(theme::WARNING),
    ))
  } else {
    let visible = app.visible_items().len();
    let total = app.items_for_tab().len();
    let gray = Style::default().fg(theme::TEXT_DIM);
    let mut spans = if app.active_filters.is_empty()
      && app.search_query.is_empty()
    {
      let label = if app.feed_tab == FeedTab::Discoveries {
        format!("{total} discoveries")
      } else {
        format!("{total} items")
      };
      vec![Span::styled(label, gray)]
    } else {
      let mut s = vec![Span::styled(format!("{visible}/{total} items"), gray)];
      if !app.active_filters.is_empty() {
        s.push(Span::styled(
          "  [filtered]",
          Style::default().fg(theme::WARNING),
        ));
      }
      s
    };
    if let Some((hint, col)) = repo_hint {
      spans.push(Span::styled(hint, Style::default().fg(col)));
    }
    Line::from(spans)
  };
  frame.render_widget(Paragraph::new(vec![stats_line]), rows[0]);

  // Row 1: single-line hint
  let gray = Style::default().fg(theme::TEXT_DIM);
  let feed_hint = if app.feed_tab == FeedTab::Discoveries {
    "Ldr+d: inbox"
  } else {
    "Ldr+d: discoveries"
  };
  let hint_line = Line::from(Span::styled(
    format!("Ldr: ctrl+t  │  {feed_hint}  │  Ldr+?: help"),
    gray,
  ));
  frame.render_widget(Paragraph::new(vec![hint_line]), rows[1]);
}

// ── Sources popup ──────────────────────────────────────────────────────────

fn draw_sources_popup(frame: &mut Frame, app: &App) {
  let area = frame.area();
  let popup_w = (area.width as u32 * 70 / 100) as u16;
  let popup_h = (area.height as u32 * 80 / 100) as u16;
  let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
  let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
  let popup_area = ratatui::layout::Rect::new(x, y, popup_w, popup_h);

  frame.render_widget(Clear, popup_area);

  let block = Block::default()
    .borders(Borders::ALL)
    .title(Span::styled(
      " Manage Sources ",
      Style::default().fg(theme::TEXT).add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::BORDER))
    .style(Style::default().bg(theme::BG_POPUP));

  let inner = block.inner(popup_area);
  frame.render_widget(block, popup_area);

  let w = inner.width as usize;
  let hrule = "─".repeat(w.saturating_sub(4));
  let cats = app.sources_popup_arxiv_cats();
  let cats_count = cats.len();
  let sources_count = crate::config::PREDEFINED_SOURCES.len();
  let custom_feeds = &app.config.sources.custom_feeds;
  let cursor = app.sources_cursor;

  let gray = Style::default().fg(theme::TEXT_DIM);
  let white = Style::default().fg(theme::TEXT);
  let bold_white =
    Style::default().fg(theme::HEADER).add_modifier(Modifier::BOLD);
  let cyan = Style::default().fg(theme::ACCENT);
  let selected_style = Style::default()
    .bg(theme::BG_SELECTION)
    .fg(theme::TEXT)
    .add_modifier(Modifier::BOLD);

  let mut lines: Vec<Line> = Vec::new();

  lines.push(Line::from(""));
  lines.push(Line::from(Span::styled("  Add source", bold_white)));

  let input_active = app.sources_input_active;
  let input_focused = cursor == 0;
  let border_col = if input_active {
    theme::SUCCESS
  } else if input_focused {
    theme::ACCENT
  } else {
    theme::BORDER
  };

  let input_display = if app.sources_input.is_empty() && !input_active {
    "paste a URL...".to_string()
  } else if input_active {
    format!("{}_", app.sources_input)
  } else {
    app.sources_input.clone()
  };
  let box_inner_w = w.saturating_sub(6);
  let n = input_display.chars().count();
  let padded = if n >= box_inner_w {
    input_display.chars().take(box_inner_w).collect::<String>()
  } else {
    format!("{}{}", input_display, " ".repeat(box_inner_w - n))
  };
  let hbar: String = "─".repeat(box_inner_w + 2);
  let bc = Style::default().fg(border_col);
  lines.push(Line::from(Span::styled(format!("  ┌{}┐", hbar), bc)));
  lines.push(Line::from(vec![
    Span::styled("  │ ", bc),
    Span::raw(padded),
    Span::styled(" │", bc),
  ]));
  lines.push(Line::from(Span::styled(format!("  └{}┘", hbar), bc)));

  let detect_line = match &app.sources_detect_state {
    SourcesDetectState::Idle => {
      if input_focused && !app.sources_input.is_empty() && !input_active {
        Line::from(Span::styled("  Press Enter to detect feed type", gray))
      } else {
        Line::from("")
      }
    }
    SourcesDetectState::Detecting => Line::from(Span::styled(
      "  Detecting...",
      Style::default().fg(theme::WARNING),
    )),
    SourcesDetectState::Result(r) => match r {
      DiscoverResult::ArxivCategory(code) => Line::from(Span::styled(
        format!("  Detected: arXiv category {code} — press Enter to confirm"),
        Style::default().fg(theme::SUCCESS),
      )),
      DiscoverResult::HuggingFaceAlreadyEnabled => Line::from(Span::styled(
        "  Detected: HuggingFace daily papers — already enabled",
        gray,
      )),
      DiscoverResult::RssFeed { url, .. } => {
        let display = truncate(url, w.saturating_sub(36));
        Line::from(Span::styled(
          format!("  Detected: RSS feed at {display} — press Enter to confirm"),
          Style::default().fg(theme::SUCCESS),
        ))
      }
      DiscoverResult::Failed(msg) => Line::from(Span::styled(
        format!("  {msg}"),
        Style::default().fg(theme::ERROR),
      )),
    },
  };
  lines.push(detect_line);
  lines.push(Line::from(""));

  lines.push(Line::from(Span::styled("  arXiv categories", bold_white)));
  lines.push(Line::from(Span::styled(format!("  {hrule}"), gray)));
  for (i, (code, label)) in cats.iter().enumerate() {
    let pos = 1 + i;
    let sel = cursor == pos;
    let enabled = app.config.sources.arxiv_categories.contains(code);
    let cb = if enabled { "[x]" } else { "[ ]" };
    let label_str =
      if label.is_empty() { code.as_str() } else { label.as_str() };
    let text = format!("  {cb} {code:<8} {label_str}");
    let style = if sel {
      selected_style
    } else if enabled {
      cyan
    } else {
      gray
    };
    lines.push(Line::from(Span::styled(text, style)));
  }
  lines.push(Line::from(""));

  lines.push(Line::from(Span::styled("  Sources", bold_white)));
  lines.push(Line::from(Span::styled(format!("  {hrule}"), gray)));
  for (i, &name) in crate::config::PREDEFINED_SOURCES.iter().enumerate() {
    let pos = 1 + cats_count + i;
    let sel = cursor == pos;
    let enabled =
      app.config.sources.enabled_sources.get(name).copied().unwrap_or(true);
    let cb = if enabled { "[x]" } else { "[ ]" };
    let text = format!("  {cb} {name}");
    let style = if sel {
      selected_style
    } else if enabled {
      cyan
    } else {
      gray
    };
    lines.push(Line::from(Span::styled(text, style)));
  }
  lines.push(Line::from(""));

  lines.push(Line::from(Span::styled("  Custom feeds", bold_white)));
  lines.push(Line::from(Span::styled(format!("  {hrule}"), gray)));
  if custom_feeds.is_empty() {
    lines.push(Line::from(Span::styled("  none", gray)));
  } else {
    for (i, feed) in custom_feeds.iter().enumerate() {
      let pos = 1 + cats_count + sources_count + i;
      let sel = cursor == pos;
      let text = format!("  [x] {}", feed.name);
      let style = if sel { selected_style } else { cyan };
      lines.push(Line::from(Span::styled(text, style)));
    }
  }
  lines.push(Line::from(""));

  lines.push(Line::from(Span::styled(
    "  j/k: navigate  space: toggle  d: delete  esc: back",
    white,
  )));

  let para = Paragraph::new(lines);
  frame.render_widget(para, inner);
}

// ── Settings view ──────────────────────────────────────────────────────────

fn draw_settings(frame: &mut Frame, app: &App) {
  let area = frame.area();

  let block = Block::default()
    .borders(Borders::ALL)
    .title(Span::styled(
      " Settings ",
      Style::default().fg(theme::TEXT).add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(theme::BORDER));

  let inner = block.inner(area);
  frame.render_widget(block, area);

  let w = inner.width as usize;
  let box_inner_w = w.saturating_sub(6);
  let hrule: String = "─".repeat(w.saturating_sub(2));
  let box_h_bar: String = "─".repeat(box_inner_w + 2);
  let box_top = format!("  ┌{}┐", box_h_bar);
  let box_bot = format!("  └{}┘", box_h_bar);

  let mask_str = |s: &str| -> String { "*".repeat(s.chars().count()) };

  let pad_to = |s: String| -> String {
    let n = s.chars().count();
    if n >= box_inner_w {
      s.chars().take(box_inner_w).collect()
    } else {
      format!("{}{}", s, " ".repeat(box_inner_w - n))
    }
  };

  let gh_display = if app.settings_editing && app.settings_field == 0 {
    pad_to(format!("{}_", mask_str(&app.settings_edit_buf)))
  } else {
    pad_to(mask_str(&app.settings_github_token))
  };
  let s2_display = if app.settings_editing && app.settings_field == 1 {
    pad_to(format!("{}_", mask_str(&app.settings_edit_buf)))
  } else {
    pad_to(mask_str(&app.settings_s2_key))
  };

  let border_color = |field: usize, editing: bool| -> Color {
    if app.settings_field == field {
      if editing { theme::SUCCESS } else { theme::ACCENT }
    } else {
      theme::BORDER
    }
  };

  let gh_col = border_color(0, app.settings_editing);
  let s2_col = border_color(1, app.settings_editing);
  let claude_col = border_color(2, app.settings_editing);
  let openai_col = border_color(3, app.settings_editing);

  let claude_display = if app.settings_editing && app.settings_field == 2 {
    pad_to(format!("{}_", mask_str(&app.settings_edit_buf)))
  } else {
    pad_to(mask_str(&app.settings_claude_key))
  };
  let openai_display = if app.settings_editing && app.settings_field == 3 {
    pad_to(format!("{}_", mask_str(&app.settings_edit_buf)))
  } else {
    pad_to(mask_str(&app.settings_openai_key))
  };
  let claude_count = {
    let n = app.settings_claude_key.chars().count();
    if n == 0 {
      "  not set".to_string()
    } else {
      format!("  {n} characters stored")
    }
  };
  let openai_count = {
    let n = app.settings_openai_key.chars().count();
    if n == 0 {
      "  not set".to_string()
    } else {
      format!("  {n} characters stored")
    }
  };

  let box_mid = |content: String, col: Color| -> Line<'static> {
    Line::from(vec![
      Span::styled("  │ ", Style::default().fg(col)),
      Span::raw(content),
      Span::styled(" │", Style::default().fg(col)),
    ])
  };

  let box_line = |s: String, col: Color| -> Line<'static> {
    Line::from(Span::styled(s, Style::default().fg(col)))
  };

  let gh_count = {
    let n = app.settings_github_token.chars().count();
    if n == 0 {
      "  not set".to_string()
    } else {
      format!("  {n} characters stored")
    }
  };
  let s2_count = {
    let n = app.settings_s2_key.chars().count();
    if n == 0 {
      "  not set".to_string()
    } else {
      format!("  {n} characters stored")
    }
  };

  let cats = app.config.sources.arxiv_categories.join(", ");
  let mut active: Vec<String> = app
    .config
    .sources
    .enabled_sources
    .iter()
    .filter(|(_, v)| **v)
    .map(|(k, _)| k.clone())
    .collect();
  active.sort();
  let active_str =
    if active.is_empty() { "none".to_string() } else { active.join(", ") };
  let custom_count = app.config.sources.custom_feeds.len();
  let custom_str = if custom_count == 0 {
    "none".to_string()
  } else {
    custom_count.to_string()
  };

  let gray = Style::default().fg(theme::TEXT_DIM);
  let white = Style::default().fg(theme::TEXT);
  let bold_white =
    Style::default().fg(theme::HEADER).add_modifier(Modifier::BOLD);

  let mut lines: Vec<Line> = vec![
    Line::from(""),
    Line::from(Span::styled("  API Keys", bold_white)),
    Line::from(Span::styled(format!("  {hrule}"), gray)),
    Line::from(""),
    Line::from(Span::styled("  GitHub token", white)),
    box_line(box_top.clone(), gh_col),
    box_mid(gh_display, gh_col),
    box_line(box_bot.clone(), gh_col),
    Line::from(Span::styled(gh_count, gray)),
    Line::from(Span::styled("  Used for the repo viewer", gray)),
    Line::from(""),
    Line::from(Span::styled("  Semantic Scholar API key", white)),
    box_line(box_top.clone(), s2_col),
    box_mid(s2_display, s2_col),
    box_line(box_bot.clone(), s2_col),
    Line::from(Span::styled(s2_count, gray)),
    Line::from(Span::styled(
      "  Improves paper metadata · semanticscholar.org/product/api",
      gray,
    )),
    Line::from(""),
    Line::from(Span::styled("  Chat", bold_white)),
    Line::from(Span::styled(format!("  {hrule}"), gray)),
    Line::from(""),
    Line::from(Span::styled("  Claude API key", white)),
    box_line(box_top.clone(), claude_col),
    box_mid(claude_display, claude_col),
    box_line(box_bot.clone(), claude_col),
    Line::from(Span::styled(claude_count, gray)),
    Line::from(Span::styled("  Used for claude: prefix in chat", gray)),
    Line::from(""),
    Line::from(Span::styled("  OpenAI API key", white)),
    box_line(box_top, openai_col),
    box_mid(openai_display, openai_col),
    box_line(box_bot, openai_col),
    Line::from(Span::styled(openai_count, gray)),
    Line::from(Span::styled("  Used for openai: prefix in chat", gray)),
    Line::from(""),
    Line::from(Span::styled("  Default chat provider", white)),
    Line::from(vec![
      Span::styled(
        "  [enter to toggle]  ",
        if app.settings_field == 4 {
          Style::default().fg(theme::ACCENT)
        } else {
          gray
        },
      ),
      Span::styled(
        app.settings_default_chat_provider.clone(),
        if app.settings_field == 4 {
          Style::default().fg(theme::SUCCESS).add_modifier(Modifier::BOLD)
        } else {
          white
        },
      ),
    ]),
    Line::from(""),
    Line::from(Span::styled("  Sources", bold_white)),
    Line::from(Span::styled(format!("  {hrule}"), gray)),
    Line::from(vec![
      Span::styled("  arXiv categories : ", gray),
      Span::styled(cats, white),
    ]),
    Line::from(vec![
      Span::styled("  Active sources   : ", gray),
      Span::styled(active_str, white),
    ]),
    Line::from(vec![
      Span::styled("  Custom feeds     : ", gray),
      Span::styled(custom_str, white),
    ]),
    Line::from(Span::styled("  p: manage sources", gray)),
    Line::from(""),
  ];

  if app.settings_save_time.is_some() {
    lines.push(Line::from(Span::styled(
      "  Saved.",
      Style::default().fg(theme::SUCCESS),
    )));
  } else {
    lines.push(Line::from(Span::styled(
      "  esc / q: back · enter: edit · s / S: save",
      gray,
    )));
  }

  let para = Paragraph::new(lines).wrap(Wrap { trim: false });
  frame.render_widget(para, inner);
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Truncate `s` to at most `max_chars` Unicode scalar values.
/// Returns a `&str` slice ending on a char boundary — never panics on multibyte input.
fn safe_truncate_chars(s: &str, max_chars: usize) -> &str {
  match s.char_indices().nth(max_chars) {
    Some((byte_idx, _)) => &s[..byte_idx],
    None => s,
  }
}

/// Shrink a rect by `margin` columns on each side (horizontal only).
fn h_margin(r: Rect, margin: u16) -> Rect {
  Rect { x: r.x + margin, width: r.width.saturating_sub(margin * 2), ..r }
}

/// Count how many items (starting from `list_offset`) fit in `viewport_rows`
/// screen rows, given that a title longer than `title_wrap_w` chars takes 2 rows.
fn count_visible_items(
  items: &[&crate::models::FeedItem],
  list_offset: usize,
  viewport_rows: usize,
  title_wrap_w: usize,
) -> usize {
  let mut rows_used = 0usize;
  let mut count = 0usize;
  for item in items.iter().skip(list_offset) {
    let item_height = if item.title.len() > title_wrap_w { 2 } else { 1 };
    if rows_used + item_height > viewport_rows {
      break;
    }
    rows_used += item_height;
    count += 1;
  }
  count.max(1)
}

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

// ── Shared-box layout helpers ─────────────────────────────────────────────

/// Draws one outer DarkGray border enclosing two side-by-side columns.
/// `right_w` is the width of the right column INSIDE the border (no border chars).
/// Draws a `│` divider between columns, `┬`/`┴` connectors at top/bottom border,
/// and title strings (` {title} ` padded with `─`) embedded in the top border row.
/// Returns `(left_inner, right_inner)` — content rects with no own borders.
fn draw_horiz_split_box(
  frame: &mut Frame,
  area: Rect,
  right_w: u16,
  left_title: &str,
  right_title: &str,
) -> (Rect, Rect) {
  let s = Style::default().fg(theme::BORDER);

  // Outer border (provides ┌┐└┘ and ─/│ edges)
  frame.render_widget(
    Block::default().borders(Borders::ALL).border_style(s),
    area,
  );

  // Inner content rect
  let inner = Rect {
    x: area.x + 1,
    y: area.y + 1,
    width: area.width.saturating_sub(2),
    height: area.height.saturating_sub(2),
  };

  // Clamp right_w so there is always at least 1 column on each side
  let right_w = right_w.min(inner.width.saturating_sub(2));
  let left_w = inner.width.saturating_sub(right_w + 1); // +1 for divider col
  let div_x = inner.x + left_w;

  // Vertical divider body
  if inner.height > 0 {
    let div_lines: Vec<Line> =
      (0..inner.height).map(|_| Line::from(Span::styled("│", s))).collect();
    frame.render_widget(
      Paragraph::new(div_lines),
      Rect { x: div_x, y: inner.y, width: 1, height: inner.height },
    );
  }

  // ┬ / ┴ connectors
  frame.render_widget(
    Paragraph::new(Span::styled("┬", s)),
    Rect { x: div_x, y: area.y, width: 1, height: 1 },
  );
  if area.height > 1 {
    frame.render_widget(
      Paragraph::new(Span::styled("┴", s)),
      Rect { x: div_x, y: area.y + area.height - 1, width: 1, height: 1 },
    );
  }

  // Title overlays on the top border row
  if left_w > 0 {
    let t = format!("{:─^w$}", format!(" {left_title} "), w = left_w as usize);
    frame.render_widget(
      Paragraph::new(Span::styled(t, s)),
      Rect { x: area.x + 1, y: area.y, width: left_w, height: 1 },
    );
  }
  if right_w > 0 {
    let t =
      format!("{:─^w$}", format!(" {right_title} "), w = right_w as usize);
    frame.render_widget(
      Paragraph::new(Span::styled(t, s)),
      Rect { x: div_x + 1, y: area.y, width: right_w, height: 1 },
    );
  }

  let left_rect =
    Rect { x: inner.x, y: inner.y, width: left_w, height: inner.height };
  let right_rect =
    Rect { x: div_x + 1, y: inner.y, width: right_w, height: inner.height };
  (left_rect, right_rect)
}

/// Draws one outer DarkGray border enclosing two vertically stacked rows.
/// The top section title is embedded in the top border; the bottom section
/// title is embedded in a `├─ Title ─┤` divider row between the sections.
/// Returns `(top_inner, bottom_inner)` — content rects with no own borders.
fn draw_vert_split_box(
  frame: &mut Frame,
  area: Rect,
  top_title: &str,
  bottom_title: &str,
) -> (Rect, Rect) {
  let s = Style::default().fg(theme::BORDER);

  frame.render_widget(
    Block::default().borders(Borders::ALL).border_style(s),
    area,
  );

  let inner = Rect {
    x: area.x + 1,
    y: area.y + 1,
    width: area.width.saturating_sub(2),
    height: area.height.saturating_sub(2),
  };

  // Split evenly; divider row is 1 row
  let top_h = (inner.height / 2).max(3).min(inner.height.saturating_sub(2));
  let div_y = inner.y + top_h;
  let bot_h = inner.height.saturating_sub(top_h + 1);

  // ├─ Bottom title ─┤ divider row
  let div_content =
    format!("{:─^w$}", format!(" {bottom_title} "), w = inner.width as usize);
  let div_line = format!("├{div_content}┤");
  frame.render_widget(
    Paragraph::new(Span::styled(div_line, s)),
    Rect { x: area.x, y: div_y, width: area.width, height: 1 },
  );

  // Top title overlay in top border row
  if inner.width > 0 {
    let t =
      format!("{:─^w$}", format!(" {top_title} "), w = inner.width as usize);
    frame.render_widget(
      Paragraph::new(Span::styled(t, s)),
      Rect { x: area.x + 1, y: area.y, width: inner.width, height: 1 },
    );
  }

  let top_rect =
    Rect { x: inner.x, y: inner.y, width: inner.width, height: top_h };
  let bot_rect =
    Rect { x: inner.x, y: div_y + 1, width: inner.width, height: bot_h };
  (top_rect, bot_rect)
}

// ── Help overlay ─────────────────────────────────────────────────────────────

const HELP_SECTIONS: &[(&str, &[(&str, &str)])] = &[
  (
    "Navigation",
    &[
      ("j / k", "Move down / up in feed"),
      ("g / G", "Jump to top / bottom"),
      ("PgDn / PgUp", "Scroll details ±10 lines"),
      ("Tab", "Focus filter panel"),
      ("enter", "Open article in reader"),
      ("esc", "Close / go back"),
      ("click", "Focus any pane"),
    ],
  ),
  (
    "Leader (Ctrl+T)",
    &[
      ("Ldr+?", "Open this help screen"),
      ("Ldr+q", "Quit application"),
      ("Ldr+n", "Toggle notes panel"),
      ("Ldr+c", "Toggle chat panel"),
      ("Ldr+S", "Open settings"),
      ("Ldr+z", "Move chat panel top / bottom"),
      ("Ldr+h/l", "Focus pane left / right"),
      ("Ldr+j/k", "Focus pane down / up"),
    ],
  ),
  (
    "Feed",
    &[
      ("/", "Open search"),
      ("esc", "Clear search"),
      ("R", "Refresh all sources"),
      ("o", "Open URL in browser"),
      ("v", "Open repo viewer"),
      ("i", "Mark as Inbox"),
      ("s", "Mark as Skimmed"),
      ("r", "Mark as Queued (to read)"),
      ("w", "Mark as DeepRead"),
      ("x", "Archive"),
    ],
  ),
  (
    "Reader",
    &[
      ("vim keys", "Standard vim navigation"),
      ("q / Esc", "Close reader"),
      ("Ldr+n", "Toggle notes alongside reader"),
      ("", ""),
      ("Voice", ""),
      ("r", "Enter reading mode (or re-read paragraph)"),
      ("R", "Read from cursor to end of paragraph"),
      ("Ctrl+p", "Continuous reading (auto-advance paragraphs)"),
      ("Space", "Pause / resume playback"),
      ("c", "Re-centre view on playing paragraph"),
      ("Esc", "Stop playback and exit reading mode"),
    ],
  ),
  (
    "Chat",
    &[
      ("enter", "Send message"),
      ("j / k", "Scroll chat history"),
      ("esc", "Back to session list"),
      ("Ldr+c", "Close chat panel"),
      ("Ldr+z", "Move chat top / bottom"),
    ],
  ),
  (
    "Sources & Settings",
    &[
      ("Ldr+S", "Open settings screen"),
      ("tab", "Cycle settings fields"),
      ("enter", "Edit selected field"),
      ("esc", "Save and leave settings"),
    ],
  ),
];

fn draw_help_overlay(frame: &mut Frame, app: &mut App) {
  let area = frame.area();

  // Centered popup: 80% width, 80% height, min 40×20.
  let popup_w = ((area.width as f32 * 0.80) as u16).max(40).min(area.width);
  let popup_h = ((area.height as f32 * 0.80) as u16).max(20).min(area.height);
  let popup_x = (area.width.saturating_sub(popup_w)) / 2;
  let popup_y = (area.height.saturating_sub(popup_h)) / 2;
  let popup_rect = Rect::new(popup_x, popup_y, popup_w, popup_h);

  // Dim the background.
  frame.render_widget(Clear, popup_rect);

  let block = Block::default()
    .borders(Borders::ALL)
    .border_style(Style::default().fg(theme::BORDER))
    .title(Span::styled(
      " help ",
      Style::default().fg(theme::TEXT).add_modifier(Modifier::BOLD),
    ));
  let inner = block.inner(popup_rect);
  frame.render_widget(block, popup_rect);

  // Tab bar row.
  let tab_h = 1u16;
  let content_h = inner.height.saturating_sub(tab_h + 1); // +1 for separator

  let layout_rows = Layout::vertical([
    Constraint::Length(tab_h),
    Constraint::Length(1),
    Constraint::Min(0),
  ])
  .split(inner);
  let tab_area = layout_rows[0];
  let sep_area = layout_rows[1];
  let body_area = layout_rows[2];

  // Draw tab labels.
  let tab_style_active = Style::default()
    .fg(theme::TEXT_ON_ACCENT)
    .bg(theme::ACCENT)
    .add_modifier(Modifier::BOLD);
  let tab_style_inactive = Style::default().fg(theme::TEXT_DIM);
  let mut tab_spans: Vec<Span> = Vec::new();
  for (i, (name, _)) in HELP_SECTIONS.iter().enumerate() {
    let label = format!(" {name} ");
    if i == app.help_section {
      tab_spans.push(Span::styled(label, tab_style_active));
    } else {
      tab_spans.push(Span::styled(label, tab_style_inactive));
    }
    if i + 1 < HELP_SECTIONS.len() {
      tab_spans.push(Span::styled("│", Style::default().fg(theme::BORDER)));
    }
  }
  frame.render_widget(Paragraph::new(Line::from(tab_spans)), tab_area);

  // Separator.
  let sep = "─".repeat(sep_area.width as usize);
  frame.render_widget(
    Paragraph::new(Span::styled(sep, Style::default().fg(theme::BORDER))),
    sep_area,
  );

  // Body: keybinding table for active section.
  let (_, bindings) =
    HELP_SECTIONS[app.help_section.min(HELP_SECTIONS.len() - 1)];
  let key_col_w = 18u16;
  let desc_col_w = body_area.width.saturating_sub(key_col_w + 4); // padding

  let key_style = Style::default().fg(theme::ACCENT);
  let header_style =
    Style::default().fg(theme::HEADER).add_modifier(Modifier::UNDERLINED);
  let desc_style = Style::default().fg(theme::TEXT);
  let gray = Style::default().fg(theme::TEXT_DIM);

  let mut body_lines: Vec<Line> = vec![Line::from("")];
  for (key, desc) in bindings.iter() {
    if key.is_empty() && desc.is_empty() {
      // blank spacer row
      body_lines.push(Line::from(""));
      continue;
    }
    if !key.is_empty() && desc.is_empty() {
      // section subheading (key text, no description)
      let header_cell = format!("  {}", key);
      body_lines.push(Line::from(Span::styled(header_cell, header_style)));
      continue;
    }
    let key_cell = format!("  {:<width$}", key, width = key_col_w as usize);
    body_lines.push(Line::from(vec![
      Span::styled(key_cell, key_style),
      Span::styled(*desc, desc_style),
    ]));
  }
  body_lines.push(Line::from(""));
  body_lines.push(Line::from(Span::styled(
    "  Tab/h/l: next section  │  j/k: scroll  │  q/Esc: close",
    gray,
  )));

  let total_lines = body_lines.len() as u16;
  let max_scroll = total_lines.saturating_sub(content_h);
  app.help_scroll = app.help_scroll.min(max_scroll);
  let scroll = app.help_scroll;

  frame
    .render_widget(Paragraph::new(body_lines).scroll((scroll, 0)), body_area);
  let _ = desc_col_w; // used for layout intent; suppress unused warning
}
