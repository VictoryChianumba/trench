use ratatui::{
  layout::{Alignment, Constraint, Direction, Layout, Rect},
  style::{Color, Modifier, Style},
  text::{Line, Span, Text},
  widgets::{
    Block, Borders, Cell, Clear, Paragraph, Row, Scrollbar,
    ScrollbarOrientation, ScrollbarState, Table, Wrap,
  },
  Frame,
};

use super::repo_viewer::draw_repo_viewer;
use crate::app::{
  App, AppView, DiscoverResult, FeedTab, FocusedReader, NotesTab, PaneId,
  ReaderTab, SourcesDetectState,
};
use crate::models::{ContentType, SignalLevel, SourcePlatform, WorkflowState};
use std::collections::HashSet;

pub const RIGHT_COL_WIDTH: u16 = 50;

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
  // Abstract popup floats on top of the feed view.
  if app.abstract_popup_active {
    draw_abstract_popup(frame, app);
  }
  // Help overlay floats on top of whatever view is rendered.
  if app.help_active {
    draw_help_overlay(frame, app);
  }
  if app.theme_picker_active {
    draw_theme_picker(frame, app);
  }
  let total_ms = t_total.elapsed().as_millis();
  if total_ms > 8 {
    log::debug!("ui::draw total: {}ms", total_ms);
  }
}

fn draw_feed(frame: &mut Frame, app: &mut App) {
  let area = frame.area();
  let theme = app.active_theme.theme();
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
    draw_title_bar(frame, app, rows[0]);
    log::debug!("draw_title_bar: {}ms", t.elapsed().as_millis());

    let t = std::time::Instant::now();
    draw_search_row(frame, app, h_margin(rows[1], margin));
    log::debug!("draw_search_row: {}ms", t.elapsed().as_millis());

    let chat_rect = Some(rows[2]);
    if let Some(chat_ui) = app.chat_ui.as_mut() {
      let t = std::time::Instant::now();
      chat_ui.draw(frame, rows[2], &theme);
      log::debug!("chat_ui.draw (top): {}ms", t.elapsed().as_millis());
    }

    let t = std::time::Instant::now();
    let mr = draw_main_row(frame, app, h_margin(rows[3], margin));
    log::debug!("draw_main_row: {}ms", t.elapsed().as_millis());

    app.update_pane_rects(
      mr.feed,
      mr.reader,
      mr.notes,
      mr.details,
      chat_rect,
      mr.secondary_reader,
    );

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
    draw_title_bar(frame, app, rows[0]);
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
        chat_ui.draw(frame, rows[3], &theme);
        log::debug!("chat_ui.draw (bottom): {}ms", t.elapsed().as_millis());
      }
    }
    app.update_pane_rects(
      mr.feed,
      mr.reader,
      mr.notes,
      mr.details,
      chat_rect,
      mr.secondary_reader,
    );

    let t = std::time::Instant::now();
    draw_footer(frame, app, rows[4]);
    log::debug!("draw_footer: {}ms", t.elapsed().as_millis());
  }

  // Session-list / new-session overlay: rendered last so it floats on top.
  if app.chat_active && !chat_needs_panel {
    if let Some(chat_ui) = app.chat_ui.as_mut() {
      chat_ui.draw_overlay(frame, area, &theme);
    }
  }

  // A1 — floating reader popup (Ldr+Enter).
  if app.reader_popup_active {
    draw_reader_popup(frame, app, area);
  }

  // A2 State 3 — bottom pane visible only when summoned (Ldr+v).
  if app.reader_dual_active && app.reader_bottom_open {
    draw_reader_bottom_pane(frame, app, area);
  }
}

// ── Title bar ──────────────────────────────────────────────────────────────

fn title_bar_height(_width: u16) -> u16 {
  5
}

fn draw_title_bar(frame: &mut Frame, app: &App, area: Rect) {
  draw_compact_title_bar(frame, app, area);
}

fn draw_compact_title_bar(frame: &mut Frame, app: &App, area: Rect) {
  let t = app.active_theme.theme();
  let inner = Layout::default()
    .direction(Direction::Vertical)
    .constraints([
      Constraint::Length(1),
      Constraint::Length(1),
      Constraint::Length(1),
      Constraint::Length(1),
      Constraint::Length(1),
    ])
    .split(area);

  let width = area.width as usize;
  let queued = app
    .items
    .iter()
    .filter(|i| i.workflow_state == WorkflowState::Queued)
    .count();
  let read = app
    .items
    .iter()
    .filter(|i| i.workflow_state == WorkflowState::DeepRead)
    .count();
  let total = app.items.len();
  let active_style = Style::default().fg(t.text).add_modifier(Modifier::BOLD);
  let inactive_style = Style::default().fg(t.text_dim);
  let inbox_style =
    if app.feed_tab == FeedTab::Inbox { active_style } else { inactive_style };
  let discoveries_style = if app.feed_tab == FeedTab::Discoveries {
    active_style
  } else {
    inactive_style
  };
  const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
  let discovery_spin = if app.discovery_loading {
    format!(" {}", SPINNER[app.spinner_frame % SPINNER.len()])
  } else {
    String::new()
  };
  const WORDMARK: &[&str] = &[
    "█▀█ █▄ █ █▀  █▀█ █▀ █▀ █▀ █▀█ █▀█ █ █",
    "█ █ █ ▀█ █▀  █▀▄ █▀ ▀█ █▀ █▀█ █▀▄ █▀█",
    "▀▀▀ ▀  ▀ ▀▀  ▀ ▀ ▀▀ ▀▀ ▀▀ ▀ ▀ ▀ ▀ ▀ ▀",
  ];
  let nav_text = format!(
    "Inbox {}  Discoveries {}{}  Queue {queued}  Read {read}  Total {total}",
    app.items.len(),
    app.discovery_items.len(),
    discovery_spin
  );
  let logo_style = Style::default().fg(t.accent).add_modifier(Modifier::BOLD);
  let logo_width =
    WORDMARK.iter().map(|line| line.chars().count()).max().unwrap_or(0);
  let logo = Line::from(Span::styled(WORDMARK[0], logo_style));
  frame.render_widget(Paragraph::new(logo), inner[1]);

  let nav_width = nav_text.chars().count();
  let centered_nav_x = width.saturating_sub(nav_width) / 2;
  let nav_x = centered_nav_x.max(logo_width.saturating_add(3));
  let logo_gap = nav_x.saturating_sub(WORDMARK[1].chars().count());
  let version_gap =
    width.saturating_sub(nav_x + nav_width + VERSION.len()).max(1);
  let nav = Line::from(vec![
    Span::styled(WORDMARK[1], logo_style),
    Span::raw(" ".repeat(logo_gap)),
    Span::styled("Inbox ", inbox_style),
    Span::styled(app.items.len().to_string(), inbox_style),
    Span::styled("  Discoveries ", discoveries_style),
    Span::styled(
      format!("{}{}", app.discovery_items.len(), discovery_spin),
      discoveries_style,
    ),
    Span::styled("  Queue ", inactive_style),
    Span::styled(queued.to_string(), inactive_style),
    Span::styled("  Read ", inactive_style),
    Span::styled(read.to_string(), inactive_style),
    Span::styled("  Total ", inactive_style),
    Span::styled(total.to_string(), inactive_style),
    Span::raw(" ".repeat(version_gap)),
    Span::styled(VERSION, Style::default().fg(t.text_dim)),
  ]);
  frame.render_widget(Paragraph::new(nav), inner[2]);

  frame.render_widget(
    Paragraph::new(Line::from(Span::styled(WORDMARK[2], logo_style))),
    inner[3],
  );

  let sep_str = "─".repeat(area.width as usize);
  let sep = Paragraph::new(sep_str).style(Style::default().fg(t.border));
  frame.render_widget(sep, inner[4]);
}

// ── Search + filter row ────────────────────────────────────────────────────

fn draw_search_row(frame: &mut Frame, app: &App, area: Rect) {
  let t = app.active_theme.theme();
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
    Style::default().fg(t.text)
  } else {
    Style::default().fg(t.text_dim)
  };
  frame.render_widget(Paragraph::new(search_text).style(search_style), cols[0]);

  let filter_style = if app.filter_focus {
    Style::default().fg(t.accent)
  } else {
    Style::default().fg(t.text_dim)
  };
  frame.render_widget(
    Paragraph::new(format!(" {}", filter_summary(app))).style(filter_style),
    cols[1],
  );

  let sep = "─".repeat(area.width as usize);
  frame.render_widget(
    Paragraph::new(sep).style(Style::default().fg(t.border)),
    sep_area,
  );
}

fn filter_summary(app: &App) -> String {
  let f = &app.active_filters;
  let source_summary = if f.active_count() == 0 {
    "any".to_string()
  } else {
    summarize_strings(&f.sources)
  };
  format!(
    "source:{}  state:{}  type:{}  signal:{}",
    source_summary,
    summarize_ordered_set(
      &f.workflow_states,
      &[
        (WorkflowState::Inbox, "inbox"),
        (WorkflowState::Skimmed, "skimmed"),
        (WorkflowState::Queued, "queued"),
        (WorkflowState::DeepRead, "read"),
        (WorkflowState::Archived, "archived"),
      ],
    ),
    summarize_ordered_set(
      &f.content_types,
      &[
        (ContentType::Paper, "paper"),
        (ContentType::Article, "article"),
        (ContentType::Digest, "digest"),
        (ContentType::Thread, "thread"),
        (ContentType::Repo, "repo"),
      ],
    ),
    summarize_ordered_set(
      &f.signals,
      &[
        (SignalLevel::Primary, "primary"),
        (SignalLevel::Secondary, "secondary"),
        (SignalLevel::Tertiary, "tertiary"),
      ],
    ),
  )
}

fn summarize_strings(values: &HashSet<String>) -> String {
  if values.is_empty() {
    return "any".to_string();
  }
  let mut values: Vec<&str> = values.iter().map(String::as_str).collect();
  values.sort_unstable();
  summarize_labels(values)
}

fn summarize_ordered_set<T>(values: &HashSet<T>, order: &[(T, &'static str)]) -> String
where
  T: Eq + std::hash::Hash,
{
  if values.is_empty() {
    return "any".to_string();
  }
  let labels: Vec<&str> = order
    .iter()
    .filter_map(|(value, label)| values.contains(value).then_some(*label))
    .collect();
  summarize_labels(labels)
}

fn summarize_labels(labels: Vec<&str>) -> String {
  match labels.as_slice() {
    [] => "any".to_string(),
    [only] => (*only).to_string(),
    [first, second] => format!("{first},{second}"),
    [first, rest @ ..] => format!("{first}+{}", rest.len()),
  }
}

// ── Main row ───────────────────────────────────────────────────────────────

/// Screen rects computed by draw_main_row, passed back to app.update_pane_rects.
struct MainRowRects {
  feed: Option<Rect>,
  reader: Option<Rect>,
  secondary_reader: Option<Rect>,
  notes: Option<Rect>,
  details: Option<Rect>,
}

fn draw_main_row(frame: &mut Frame, app: &mut App, area: Rect) -> MainRowRects {
  let theme = app.active_theme.theme();
  let t = theme;
  // ── A2 State 3: dual-reader (left 50% | right 50%) ──────────────────────
  if app.reader_dual_active && app.reader_active {
    let inner_w = area.width.saturating_sub(2);
    let right_w = (inner_w / 2).max(1);
    let (left_rect, right_rect) =
      draw_horiz_split_box(frame, area, right_w, "Reader", "Reader", &t);
    {
      let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(0)])
        .split(left_rect);
      let focused = app.focused_reader == FocusedReader::Primary;
      draw_reader_tab_bar(
        frame,
        rows[0],
        &app.reader_tabs,
        app.reader_active_tab,
        focused,
        &t,
      );
      if let Some(editor) = app.reader_editor_mut() {
        editor.update_layout(rows[1]);
        cli_text_reader::draw_editor(frame, rows[1], editor);
      }
    }
    {
      let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(0)])
        .split(right_rect);
      let focused = app.focused_reader == FocusedReader::Secondary;
      draw_reader_tab_bar(
        frame,
        rows[0],
        &app.reader_secondary_tabs,
        app.reader_secondary_active_tab,
        focused,
        &t,
      );
      if let Some(editor) = app.reader_secondary_editor_mut() {
        editor.update_layout(rows[1]);
        cli_text_reader::draw_editor(frame, rows[1], editor);
      } else {
        let hint = Paragraph::new(
          "No paper loaded\n\nLdr+v → open feed · Enter to load",
        )
        .alignment(Alignment::Center)
        .style(Style::default().fg(t.text_dim));
        frame.render_widget(hint, rows[1]);
      }
    }
    return MainRowRects {
      feed: None,
      reader: Some(left_rect),
      secondary_reader: Some(right_rect),
      notes: None,
      details: None,
    };
  }

  // ── A2 State 2: feed (40%) | reader (60%) ────────────────────────────────
  if app.reader_split_active && app.reader_active {
    let inner_w = area.width.saturating_sub(2);
    let reader_w = (inner_w * 60 / 100).max(1);
    let (feed_rect, reader_rect) =
      draw_horiz_split_box(frame, area, reader_w, "Feed", "Reader", &t);
    draw_feed_pane(frame, app, feed_rect);
    {
      let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(0)])
        .split(reader_rect);
      draw_reader_tab_bar(
        frame,
        rows[0],
        &app.reader_tabs,
        app.reader_active_tab,
        true,
        &t,
      );
      if let Some(editor) = app.reader_editor_mut() {
        editor.update_layout(rows[1]);
        cli_text_reader::draw_editor(frame, rows[1], editor);
      }
    }
    if app.narrow_feed_details_open {
      draw_narrow_feed_details_popup(frame, app, reader_rect);
    }
    return MainRowRects {
      feed: Some(feed_rect),
      reader: Some(reader_rect),
      secondary_reader: None,
      notes: None,
      details: None,
    };
  }

  // ── Reader: always full-width or 60/40 split, regardless of terminal width ─
  if app.reader_active && !app.notes_active {
    let rows =
      Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(area);
    draw_reader_tab_bar(
      frame,
      rows[0],
      &app.reader_tabs,
      app.reader_active_tab,
      true,
      &t,
    );
    if let Some(editor) = app.reader_editor_mut() {
      let t = std::time::Instant::now();
      editor.update_layout(rows[1]);
      cli_text_reader::draw_editor(frame, rows[1], editor);
      log::debug!("draw_editor (full-width): {}ms", t.elapsed().as_millis());
    }
    return MainRowRects {
      feed: None,
      reader: Some(area),
      secondary_reader: None,
      notes: None,
      details: None,
    };
  }

  if app.reader_active {
    // Reader + notes: outer border, horizontal split regardless of width.
    let inner_w = area.width.saturating_sub(2);
    let notes_w = (inner_w * 40 / 100).max(1);
    let (reader_rect, notes_rect) =
      draw_horiz_split_box(frame, area, notes_w, "Reader", "Notes", &t);
    {
      let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(0)])
        .split(reader_rect);
      draw_reader_tab_bar(
        frame,
        rows[0],
        &app.reader_tabs,
        app.reader_active_tab,
        true,
        &t,
      );
      if let Some(editor) = app.reader_editor_mut() {
        let t = std::time::Instant::now();
        editor.update_layout(rows[1]);
        cli_text_reader::draw_editor(frame, rows[1], editor);
        log::debug!("draw_editor (split): {}ms", t.elapsed().as_millis());
      }
    }
    if let Some(notes_app) = app.notes_app.as_mut() {
      let t = std::time::Instant::now();
      let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(0)])
        .split(notes_rect);
      draw_notes_tab_bar(
        frame,
        rows[0],
        &app.notes_tabs,
        app.notes_active_tab,
        app.focused_pane == PaneId::Notes,
        &theme,
      );
      notes::draw(frame, rows[1], notes_app, &theme);
      log::debug!("notes::draw: {}ms", t.elapsed().as_millis());
    }
    return MainRowRects {
      feed: None,
      reader: Some(reader_rect),
      secondary_reader: None,
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
      draw_vert_split_box(frame, area, "Feed", bottom_title, &t);

    let t = std::time::Instant::now();
    draw_feed_pane(frame, app, feed_rect);
    log::debug!("draw_item_table (narrow): {}ms", t.elapsed().as_millis());

    let mut details_rect: Option<Rect> = None;
    if app.notes_active {
      if let Some(notes_app) = app.notes_app.as_mut() {
        let t = std::time::Instant::now();
        let rows =
          Layout::vertical([Constraint::Length(1), Constraint::Min(0)])
            .split(bottom_rect);
        draw_notes_tab_bar(
          frame,
          rows[0],
          &app.notes_tabs,
          app.notes_active_tab,
          app.focused_pane == PaneId::Notes,
          &theme,
        );
        notes::draw(frame, rows[1], notes_app, &theme);
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
      secondary_reader: None,
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
    draw_horiz_split_box(frame, area, right_w, "Feed", right_title, &t);

  let t = std::time::Instant::now();
  draw_feed_pane(frame, app, feed_rect);
  log::debug!("draw_item_table: {}ms", t.elapsed().as_millis());

  let mut details_rect: Option<Rect> = None;
  if app.notes_active {
    if let Some(notes_app) = app.notes_app.as_mut() {
      let t = std::time::Instant::now();
      let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(0)])
        .split(right_rect);
      draw_notes_tab_bar(
        frame,
        rows[0],
        &app.notes_tabs,
        app.notes_active_tab,
        app.focused_pane == PaneId::Notes,
        &theme,
      );
      notes::draw(frame, rows[1], notes_app, &theme);
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
    secondary_reader: None,
    notes: if app.notes_active { Some(right_rect) } else { None },
    details: details_rect,
  }
}

fn draw_feed_pane(frame: &mut Frame, app: &mut App, area: Rect) {
  if area.height == 0 {
    return;
  }
  let content_area = area;

  // Discoveries tab: show plan checklist or query input instead of the feed table.
  if app.feed_tab == FeedTab::Discoveries {
    if app.discovery_plan.is_some() {
      draw_discovery_checklist(frame, app, content_area);
      return;
    }
    if app.discovery_query_active {
      draw_discovery_query_bar(frame, app, content_area);
      return;
    }
  }

  // Narrow pane: switch to title-only list to avoid squished columns.
  if area.width < 70 {
    draw_narrow_feed(frame, app, content_area);
  } else {
    draw_item_table(frame, app, content_area);
  }
}

fn draw_discovery_query_bar(frame: &mut Frame, app: &App, area: Rect) {
  let t = app.active_theme.theme();
  let label = "Search: ";
  let cursor = "_";
  let query_line = Line::from(vec![
    Span::styled(label, Style::default().fg(t.text_dim)),
    Span::styled(
      format!("{}{}", app.discovery_query, cursor),
      Style::default().fg(t.text),
    ),
  ]);
  let bar_h = 2.min(area.height);
  let bar_rect =
    Rect { x: area.x, y: area.y, width: area.width, height: bar_h };
  frame.render_widget(
    Paragraph::new(vec![
      query_line,
      Line::from(Span::styled(
        "Enter: search  Esc: cancel",
        Style::default().fg(t.text_dim),
      )),
    ]),
    bar_rect,
  );
}

fn draw_discovery_checklist(frame: &mut Frame, app: &App, area: Rect) {
  let t = app.active_theme.theme();
  let plan = match &app.discovery_plan {
    Some(p) => p,
    None => return,
  };

  let cursor = app.discovery_plan_cursor;
  let w = area.width as usize;
  let mut lines: Vec<Line> = Vec::new();

  // Header
  let header_text = format!("Discovery: \"{}\"", plan.topic);
  lines.push(Line::from(Span::styled(
    truncate(&header_text, w),
    Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
  )));
  if !plan.summary.is_empty() {
    lines.push(Line::from(Span::styled(
      truncate(&plan.summary, w),
      Style::default().fg(t.text_dim),
    )));
  }
  lines.push(Line::from(Span::styled(
    "─".repeat(w),
    Style::default().fg(t.border),
  )));

  // arXiv category rows
  if !plan.arxiv_categories.is_empty() {
    lines.push(Line::from(Span::styled(
      "arXiv categories",
      Style::default().fg(t.header).add_modifier(Modifier::BOLD),
    )));
    for (i, cat) in plan.arxiv_categories.iter().enumerate() {
      let checked =
        app.discovery_plan_selected.get(i).copied().unwrap_or(false);
      let is_cursor = i == cursor;
      let cb = if checked { "[x]" } else { "[ ]" };
      let text = format!("  {cb} {cat}");
      let style = if is_cursor {
        Style::default()
          .bg(t.bg_selection)
          .fg(t.text)
          .add_modifier(Modifier::BOLD)
      } else if checked {
        Style::default().fg(t.accent)
      } else {
        Style::default().fg(t.text_dim)
      };
      lines.push(Line::from(Span::styled(truncate(&text, w), style)));
    }
    lines.push(Line::from(""));
  }

  // RSS feed rows
  let n_cats = plan.arxiv_categories.len();
  if !plan.rss_urls.is_empty() {
    lines.push(Line::from(Span::styled(
      "RSS feeds",
      Style::default().fg(t.header).add_modifier(Modifier::BOLD),
    )));
    for (j, feed) in plan.rss_urls.iter().enumerate() {
      let idx = n_cats + j;
      let checked =
        app.discovery_plan_selected.get(idx).copied().unwrap_or(false);
      let is_cursor = idx == cursor;
      let cb = if checked { "[x]" } else { "[ ]" };
      let reason = if feed.reason.is_empty() {
        String::new()
      } else {
        format!("  — {}", feed.reason)
      };
      let text = format!("  {cb} {}{}", feed.name, reason);
      let style = if is_cursor {
        Style::default()
          .bg(t.bg_selection)
          .fg(t.text)
          .add_modifier(Modifier::BOLD)
      } else if checked {
        Style::default().fg(t.accent)
      } else {
        Style::default().fg(t.text_dim)
      };
      lines.push(Line::from(Span::styled(truncate(&text, w), style)));
    }
    lines.push(Line::from(""));
  }

  // Hint line pinned at bottom if space allows; otherwise append inline.
  let hint = Line::from(Span::styled(
    "j/k: navigate  Space: toggle  a: add selected  Esc: dismiss",
    Style::default().fg(t.text_dim),
  ));
  let total_lines = lines.len() + 1; // +1 for hint
  let height = area.height as usize;

  let (scroll, show_hint_inline) = if total_lines <= height {
    (0usize, true)
  } else {
    // Scroll to keep cursor row visible.
    let cursor_row = lines
      .iter()
      .enumerate()
      .find(|(_, l)| {
        l.spans.first().map_or(false, |s| s.style.bg == Some(t.bg_selection))
      })
      .map(|(i, _)| i)
      .unwrap_or(0);
    let raw_scroll = if cursor_row >= height.saturating_sub(2) {
      cursor_row.saturating_sub(height.saturating_sub(3))
    } else {
      0
    };
    // Cap so we never scroll past the last line — no blank space at bottom.
    let scroll = raw_scroll.min(total_lines.saturating_sub(height));
    (scroll, false)
  };

  if show_hint_inline {
    lines.push(hint);
  }

  let para = Paragraph::new(lines).scroll((scroll as u16, 0));
  frame.render_widget(para, area);

  if !show_hint_inline && area.height > 0 {
    let hint_area = Rect {
      x: area.x,
      y: area.y + area.height - 1,
      width: area.width,
      height: 1,
    };
    frame.render_widget(
      Paragraph::new(Line::from(Span::styled(
        "j/k: navigate  Space: toggle  a: add selected  Esc: dismiss",
        Style::default().fg(t.text_dim),
      ))),
      hint_area,
    );
  }
}

fn draw_narrow_feed(frame: &mut Frame, app: &mut App, area: Rect) {
  let t = app.active_theme.theme();
  let viewport_rows = area.height as usize;
  let selected = app.active_selected_index();
  // Reserve space for prefix (2) + " — Type — Date" suffix (~20 chars).
  let suffix_w = 20usize;
  let prefix_w = 2usize;
  let title_w =
    (area.width as usize).saturating_sub(prefix_w + suffix_w).max(10);

  // Compute scroll offset in a scoped borrow; items can wrap to 2 rows so we
  // cannot use `viewport_rows` as the item count — use count_visible_items.
  let mut offset = app.active_list_offset();
  {
    let visible = app.visible_items();
    let total = visible.len();
    if selected < offset {
      offset = selected;
    } else {
      let vc = count_visible_items(&visible, offset, viewport_rows, title_w);
      if selected >= offset + vc {
        // Walk backward from selected accumulating row heights to find new offset.
        let mut rows = 0usize;
        offset = selected;
        for i in (0..=selected).rev() {
          let h = if visible[i].title.len() > title_w { 2 } else { 1 };
          if rows + h > viewport_rows {
            break;
          }
          rows += h;
          offset = i;
        }
      }
    }
    offset = offset.min(total.saturating_sub(1));
  }
  app.set_active_list_offset(offset);

  let visible = app.visible_items();
  let mut y = area.y;
  for (abs_i, item) in visible.iter().enumerate().skip(offset) {
    if y >= area.y + area.height {
      break;
    }
    let is_selected = abs_i == selected;
    let type_label = item.content_type.short_label();
    // Short date: first 7 chars of published_at (YYYY-MM).
    let date = &item.published_at[..item.published_at.len().min(7)];
    let suffix = format!(" — {type_label} — {date}");

    let wrapped = textwrap::wrap(&item.title, title_w);
    let line_count = wrapped.len().min(2);
    for (li, line) in wrapped.iter().take(line_count).enumerate() {
      if y >= area.y + area.height {
        break;
      }
      // Bullet only on the first line; continuation lines indent to match.
      let prefix = if li == 0 {
        if is_selected {
          "▶ "
        } else {
          "  "
        }
      } else {
        "  "
      };
      let text = if li == line_count - 1 {
        format!("{prefix}{line}{suffix}")
      } else {
        format!("{prefix}{line}")
      };
      let style = if is_selected {
        t.style_selection()
      } else {
        Style::default().fg(t.text_dim)
      };
      let row_rect = Rect { x: area.x, y, width: area.width, height: 1 };
      frame.render_widget(Paragraph::new(text).style(style), row_rect);
      y += 1;
    }
  }
}

fn draw_reader_tab_bar(
  frame: &mut Frame,
  area: Rect,
  tabs: &[ReaderTab],
  active: usize,
  focused: bool,
  t: &crate::theme::Theme,
) {
  if tabs.is_empty() {
    return;
  }
  let max_title =
    (area.width as usize).saturating_sub(4).max(8) / tabs.len().max(1);
  let spans: Vec<Span> = tabs
    .iter()
    .enumerate()
    .flat_map(|(i, tab)| {
      let title: String =
        tab.title.chars().take(max_title.saturating_sub(5)).collect();
      let label = format!("[{}: {}]", i + 1, title);
      let style = if i == active {
        if focused {
          Style::default().fg(t.accent).add_modifier(Modifier::BOLD)
        } else {
          Style::default().fg(t.text).add_modifier(Modifier::BOLD)
        }
      } else {
        Style::default().fg(t.text_dim)
      };
      let sep = Span::raw("  ");
      vec![Span::styled(label, style), sep]
    })
    .collect();
  frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_notes_tab_bar(
  frame: &mut Frame,
  area: Rect,
  tabs: &[NotesTab],
  active: usize,
  focused: bool,
  t: &crate::theme::Theme,
) {
  if tabs.is_empty() {
    return;
  }
  let max_title =
    (area.width as usize).saturating_sub(4).max(8) / tabs.len().max(1);
  let spans: Vec<Span> = tabs
    .iter()
    .enumerate()
    .flat_map(|(i, tab)| {
      let title: String =
        tab.title.chars().take(max_title.saturating_sub(5)).collect();
      let label = format!("[{}: {}]", i + 1, title);
      let style = if i == active {
        if focused {
          Style::default().fg(t.accent).add_modifier(Modifier::BOLD)
        } else {
          Style::default().fg(t.text).add_modifier(Modifier::BOLD)
        }
      } else {
        Style::default().fg(t.text_dim)
      };
      vec![Span::styled(label, style), Span::raw("  ")]
    })
    .collect();
  frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_item_table(frame: &mut Frame, app: &mut App, area: Rect) {
  let t = app.active_theme.theme();
  let t_item_table = std::time::Instant::now();
  let header_style = Style::default().fg(t.header).add_modifier(Modifier::BOLD);

  let header = Row::new(vec![
    feed_header_cell(" ", header_style),
    feed_header_cell("Src", header_style),
    feed_header_cell("Kind", header_style),
    feed_header_cell("Title", header_style),
    feed_header_cell("Author", header_style),
    feed_header_cell("Date", header_style),
    feed_header_cell("State", header_style),
  ])
  .height(2);

  // Inner area: leave one quiet row below the pane title before table headers.
  let inner = Rect {
    y: area.y.saturating_add(1),
    height: area.height.saturating_sub(1),
    ..area
  };
  if inner.height == 0 {
    return;
  }

  // Available width for title column: total inner width minus fixed cols.
  // sig(1) + source(7) + kind(5) + author(14) + date(10) + state(8) + spacing(6)
  let title_col_w =
    (inner.width.saturating_sub(1 + 7 + 5 + 14 + 10 + 8 + 6)) as usize;
  let title_wrap_w = title_col_w.max(10);

  // Viewport height in rows (inner height minus 2 header rows).
  let viewport_rows = inner.height.saturating_sub(2) as usize;

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
      let (content_height, title_lines) = &window_data[i];

      let signal_style = match item.signal {
        crate::models::SignalLevel::Primary => Style::default().fg(t.accent),
        crate::models::SignalLevel::Secondary => {
          Style::default().fg(t.text_dim)
        }
        crate::models::SignalLevel::Tertiary => Style::default().fg(t.border),
      };

      let row_style =
        if is_selected { t.style_selection() } else { Style::default() };
      let selected_text_style = t.style_selection_text();
      let selected_dim_style = t.style_selection_dim();

      let author =
        truncate(item.authors.first().map(|s| s.as_str()).unwrap_or(""), 13);

      let row_height = content_height + 1;

      Row::new(vec![
        feed_cell(
          item.signal.indicator(),
          if is_selected { selected_text_style } else { signal_style },
          is_selected,
        ),
        feed_cell(
          &feed_source_label(item),
          if is_selected {
            selected_text_style
          } else {
            Style::default().fg(t.accent)
          },
          is_selected,
        ),
        feed_cell(
          item.content_type.short_label(),
          if is_selected {
            selected_dim_style
          } else {
            Style::default().fg(t.text_dim)
          },
          is_selected,
        ),
        Cell::from(Text::from(feed_title_lines(
          style_feed_title_lines(title_lines.clone(), if is_selected {
            selected_text_style
          } else {
            Style::default()
          }),
          is_selected,
        ))),
        feed_cell(
          &author,
          if is_selected {
            selected_dim_style
          } else {
            Style::default().fg(t.text_dim)
          },
          is_selected,
        ),
        feed_cell(
          item.published_at.as_str(),
          if is_selected {
            selected_dim_style
          } else {
            Style::default().fg(t.text_dim)
          },
          is_selected,
        ),
        feed_cell(
          item.workflow_state.short_label(),
          if is_selected {
            selected_dim_style
          } else {
            Style::default().fg(t.text_dim)
          },
          is_selected,
        ),
      ])
      .style(row_style)
      .height(row_height)
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
      Constraint::Length(7),
      Constraint::Length(5),
      Constraint::Min(0),
      Constraint::Length(14),
      Constraint::Length(10),
      Constraint::Length(8),
    ],
  )
  .header(header)
  .column_spacing(1)
  .row_highlight_style(Style::default());

  let t_render = std::time::Instant::now();
  frame.render_widget(table, inner);
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

fn feed_source_label(item: &crate::models::FeedItem) -> String {
  match item.source_platform {
    SourcePlatform::HuggingFace => "hf".to_string(),
    SourcePlatform::ArXiv => "arxiv".to_string(),
    SourcePlatform::Rss if !item.source_name.is_empty() => {
      truncate(&item.source_name, 7)
    }
    _ if !item.source_name.is_empty() => truncate(&item.source_name, 7),
    _ => item.source_platform.short_label().to_string(),
  }
}

fn feed_header_cell(label: &'static str, style: Style) -> Cell<'static> {
  Cell::from(Text::from(vec![
    Line::from(Span::styled(label, style)),
    Line::from(""),
  ]))
}

fn feed_cell(value: &str, style: Style, _selected: bool) -> Cell<'static> {
  let mut lines = Vec::new();
  lines.push(Line::from(Span::styled(value.to_string(), style)));
  lines.push(Line::from(""));
  Cell::from(Text::from(lines))
}

fn feed_title_lines(
  mut lines: Vec<Line<'static>>,
  _selected: bool,
) -> Vec<Line<'static>> {
  lines.push(Line::from(""));
  lines
}

fn style_feed_title_lines(
  lines: Vec<Line<'static>>,
  style: Style,
) -> Vec<Line<'static>> {
  lines
    .into_iter()
    .map(|line| {
      Line::from(
        line
          .spans
          .into_iter()
          .map(|span| Span::styled(span.content.into_owned(), style))
          .collect::<Vec<_>>(),
      )
    })
    .collect()
}

fn draw_filter_panel(frame: &mut Frame, app: &App, area: Rect) {
  let t = app.active_theme.theme();
  let inner = area;
  let focused = app.filter_focus;

  let f = &app.active_filters;
  let c = app.filter_cursor;
  let mut s: usize = 0;
  let mut lines: Vec<Line> = Vec::new();
  let mut cursor_line: usize = 0;

  let hrule = "\u{2500}".repeat(inner.width as usize);

  lines.push(filter_header("Source", &t));
  for name in app.filter_source_names() {
    let active = f.sources.contains(&name);
    let cursor = focused && s == c;
    if cursor {
      cursor_line = lines.len();
    }
    let checkbox = if active { "[x]" } else { "[ ]" };
    let line = if cursor {
      let hl = Style::default()
        .bg(t.bg_selection)
        .fg(t.text)
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
        Span::styled(checkbox, Style::default().fg(t.text)),
        Span::raw(" "),
        Span::styled(name, Style::default().fg(t.text)),
      ])
    } else {
      Line::from(vec![
        Span::raw("  "),
        Span::styled(checkbox, Style::default().fg(t.text_dim)),
        Span::raw(" "),
        Span::raw(name),
      ])
    };
    lines.push(line);
    s += 1;
  }
  lines.push(Line::from(""));

  lines.push(filter_header("Signal", &t));
  if focused && s == c {
    cursor_line = lines.len();
  }
  lines.push(filter_row(
    "primary",
    f.signals.contains(&SignalLevel::Primary),
    focused && s == c,
    &t,
  ));
  s += 1;
  if focused && s == c {
    cursor_line = lines.len();
  }
  lines.push(filter_row(
    "secondary",
    f.signals.contains(&SignalLevel::Secondary),
    focused && s == c,
    &t,
  ));
  s += 1;
  if focused && s == c {
    cursor_line = lines.len();
  }
  lines.push(filter_row(
    "tertiary",
    f.signals.contains(&SignalLevel::Tertiary),
    focused && s == c,
    &t,
  ));
  s += 1;
  lines.push(Line::from(""));

  lines.push(filter_header("Type", &t));
  if focused && s == c {
    cursor_line = lines.len();
  }
  lines.push(filter_row(
    "paper",
    f.content_types.contains(&ContentType::Paper),
    focused && s == c,
    &t,
  ));
  s += 1;
  if focused && s == c {
    cursor_line = lines.len();
  }
  lines.push(filter_row(
    "article",
    f.content_types.contains(&ContentType::Article),
    focused && s == c,
    &t,
  ));
  s += 1;
  if focused && s == c {
    cursor_line = lines.len();
  }
  lines.push(filter_row(
    "digest",
    f.content_types.contains(&ContentType::Digest),
    focused && s == c,
    &t,
  ));
  s += 1;
  lines.push(Line::from(""));

  lines.push(filter_header("State", &t));
  if focused && s == c {
    cursor_line = lines.len();
  }
  lines.push(filter_row(
    "inbox",
    f.workflow_states.contains(&WorkflowState::Inbox),
    focused && s == c,
    &t,
  ));
  s += 1;
  if focused && s == c {
    cursor_line = lines.len();
  }
  lines.push(filter_row(
    "skimmed",
    f.workflow_states.contains(&WorkflowState::Skimmed),
    focused && s == c,
    &t,
  ));
  s += 1;
  if focused && s == c {
    cursor_line = lines.len();
  }
  lines.push(filter_row(
    "queued",
    f.workflow_states.contains(&WorkflowState::Queued),
    focused && s == c,
    &t,
  ));
  s += 1;
  if focused && s == c {
    cursor_line = lines.len();
  }
  lines.push(filter_row(
    "read",
    f.workflow_states.contains(&WorkflowState::DeepRead),
    focused && s == c,
    &t,
  ));
  s += 1;
  if focused && s == c {
    cursor_line = lines.len();
  }
  lines.push(filter_row(
    "archived",
    f.workflow_states.contains(&WorkflowState::Archived),
    focused && s == c,
    &t,
  ));
  s += 1;

  lines.push(Line::from(Span::styled(hrule, Style::default().fg(t.border))));

  let clear_hl = focused && s == c;
  if clear_hl {
    cursor_line = lines.len();
  }
  let clear_style = if clear_hl {
    Style::default().bg(t.bg_selection).fg(t.text).add_modifier(Modifier::BOLD)
  } else {
    Style::default().fg(t.text_dim)
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
  let t = app.active_theme.theme();
  let t_details = std::time::Instant::now();

  let bottom_h = area.height * 38 / 100;
  let top_h = area.height.saturating_sub(bottom_h + 1);
  let div_y = area.y + top_h;

  let top_area = Rect { height: top_h, ..area };
  let bottom_area = Rect { y: div_y + 1, height: bottom_h, ..area };

  // Divider between selected-paper detail and the activity summary.
  let sb = Style::default().fg(t.border);
  let activity_title = " Activity ";
  let activity_w = activity_title.chars().count();
  let left_rule_w = (area.width as usize).saturating_sub(activity_w) / 2;
  let right_rule_w =
    (area.width as usize).saturating_sub(activity_w + left_rule_w);
  frame.render_widget(
    Paragraph::new(Line::from(vec![
      Span::styled("─".repeat(left_rule_w), sb),
      Span::styled(
        activity_title,
        Style::default().fg(t.header).add_modifier(Modifier::BOLD),
      ),
      Span::styled("─".repeat(right_rule_w), sb),
    ])),
    Rect { x: area.x, y: div_y, width: area.width, height: 1 },
  );
  frame.render_widget(
    Paragraph::new(Span::styled("├", sb)),
    Rect { x: area.x.saturating_sub(1), y: div_y, width: 1, height: 1 },
  );
  frame.render_widget(
    Paragraph::new(Span::styled("┤", sb)),
    Rect { x: area.x + area.width, y: div_y, width: 1, height: 1 },
  );

  // ── Dashboard (bottom pane) ───────────────────────────────────────────────
  {
    let dash_inner = Rect {
      x: bottom_area.x + 1,
      width: bottom_area.width.saturating_sub(2),
      ..bottom_area
    };
    let w = dash_inner.width as usize;

    let inbox = app
      .items
      .iter()
      .filter(|i| i.workflow_state == WorkflowState::Inbox)
      .count();
    let queued = app
      .items
      .iter()
      .filter(|i| i.workflow_state == WorkflowState::Queued)
      .count();
    let read = app
      .items
      .iter()
      .filter(|i| i.workflow_state == WorkflowState::DeepRead)
      .count();
    let total = app.items.len();

    let activity_label_style =
      Style::default().fg(t.text_dim).add_modifier(Modifier::BOLD);
    let label_style = Style::default().fg(t.text_dim);
    let val_style = Style::default().fg(t.text);

    // Continue Reading
    let continue_title =
      app.last_read.as_deref().unwrap_or("─ nothing opened yet ─");
    let continue_source = app.last_read_source.as_deref().unwrap_or("");

    // Your Queue: first two queued paper titles
    let queue_items: Vec<&str> = app
      .items
      .iter()
      .filter(|i| i.workflow_state == WorkflowState::Queued)
      .map(|i| i.title.as_str())
      .take(2)
      .collect();

    // Recent (last 48 h): items published today or yesterday
    let (recent_count, today_count, recent_hf, recent_arxiv, recent_other) = {
      let today = crate::store::enrichment_cache::today_str();
      let yesterday = (chrono::Utc::now() - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
      let mut recent_count = 0;
      let mut today_count = 0;
      let mut recent_hf = 0;
      let mut recent_arxiv = 0;
      let mut recent_other = 0;
      for item in app
        .items
        .iter()
        .filter(|i| i.published_at == today || i.published_at == yesterday)
      {
        recent_count += 1;
        if item.published_at == today {
          today_count += 1;
        }
        match item.source_platform {
          SourcePlatform::HuggingFace => recent_hf += 1,
          SourcePlatform::ArXiv => recent_arxiv += 1,
          _ => recent_other += 1,
        }
      }
      (recent_count, today_count, recent_hf, recent_arxiv, recent_other)
    };

    let label_w = 11;
    let value_w = w.saturating_sub(label_w).max(1);
    let queue_summary = format!(
      "{queued} queued item{}",
      if queued == 1 { "" } else { "s" }
    );
    let fresh_summary = format!("{today_count} today   {recent_count} in 48h");
    let source_summary =
      format!("HF {recent_hf}   arXiv {recent_arxiv}   Other {recent_other}");

    let mut lines: Vec<Line> = vec![Line::from("")];
    push_activity_wrapped(
      &mut lines,
      "Last read",
      continue_title,
      activity_label_style,
      val_style,
      value_w,
      2,
    );
    if !continue_source.is_empty() {
      push_activity_continuation(
        &mut lines,
        continue_source,
        label_style,
        value_w,
        1,
      );
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
      Span::styled("Queue      ", activity_label_style),
      Span::styled(queue_summary, val_style),
    ]));
    if queue_items.is_empty() {
      lines.push(Line::from(vec![
        Span::styled("           ", label_style),
        Span::styled("─ empty ─", label_style),
      ]));
    } else {
      for title in queue_items {
        push_activity_continuation(&mut lines, title, val_style, value_w, 2);
      }
    }
    lines.extend([
      Line::from(""),
      Line::from(vec![
        Span::styled("Fresh      ", activity_label_style),
        Span::styled(truncate(&fresh_summary, value_w), val_style),
      ]),
      Line::from(vec![
        Span::styled("           ", label_style),
        Span::styled(truncate(&source_summary, value_w), label_style),
      ]),
      Line::from(""),
      Line::from(vec![
        Span::styled("Library    ", activity_label_style),
        Span::styled(
          truncate(&format!("Inbox {inbox}   Queue {queued}"), value_w),
          val_style,
        ),
      ]),
      Line::from(vec![
        Span::styled("           ", label_style),
        Span::styled(
          truncate(&format!("Read {read}   Total {total}"), value_w),
          val_style,
        ),
      ]),
    ]);

    frame.render_widget(Paragraph::new(lines), dash_inner);
  }

  // Add margin so text doesn't abut the divider.
  let inner = Rect {
    x: top_area.x + 2,
    y: top_area.y.saturating_add(1),
    width: top_area.width.saturating_sub(3),
    height: top_area.height.saturating_sub(1),
    ..top_area
  };

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

    let source_label = if item.source_name.is_empty() {
      item.source_platform.short_label().to_string()
    } else {
      item.source_name.clone()
    };
    let meta_summary = format!(
      "{} · {} · {} · {}",
      source_label,
      item.content_type.short_label(),
      item.published_at,
      item.workflow_state.short_label()
    );

    let title_style = Style::default().fg(t.text).add_modifier(Modifier::BOLD);
    let label_style = Style::default().fg(t.text_dim);
    let value_style = Style::default().fg(t.text);
    let detail_w = inner.width.max(1) as usize;
    let mut lines: Vec<Line> = textwrap::wrap(&item.title, detail_w)
      .into_iter()
      .take(2)
      .map(|line| Line::from(Span::styled(line.into_owned(), title_style)))
      .collect();
    lines.push(Line::from(Span::styled(
      truncate(&meta_summary, detail_w),
      label_style,
    )));
    lines.push(Line::from(""));
    push_detail_field(
      &mut lines,
      "Authors",
      &authors,
      label_style,
      value_style,
      detail_w,
      2,
    );

    let source_meta =
      format!("{}   {}", source_label, item.content_type.short_label());
    push_detail_field(
      &mut lines,
      "Source",
      &source_meta,
      label_style,
      Style::default().fg(t.accent),
      detail_w,
      1,
    );
    if item.source_platform == SourcePlatform::HuggingFace && item.upvote_count > 0 {
      push_detail_field(
        &mut lines,
        "Votes",
        &item.upvote_count.to_string(),
        label_style,
        value_style,
        detail_w,
        1,
      );
    }

    if let Some(ref repo) = item.github_repo {
      let display = repo.strip_prefix("https://").unwrap_or(repo.as_str());
      push_detail_field(
        &mut lines,
        "Repo",
        display,
        label_style,
        Style::default().fg(t.accent),
        detail_w,
        1,
      );
    }

    push_detail_field(
      &mut lines,
      "Tags",
      &tags,
      label_style,
      value_style,
      detail_w,
      1,
    );

    if !item.benchmark_results.is_empty() {
      lines.push(Line::from(""));
      lines.push(Line::from(Span::styled(
        "Benchmarks",
        Style::default().fg(t.header).add_modifier(Modifier::UNDERLINED),
      )));
      for b in item.benchmark_results.iter().take(3) {
        lines.push(Line::from(Span::styled(
          truncate(
            &format!("  {}/{}: {} ({})", b.task, b.dataset, b.score, b.metric),
            inner.width as usize,
          ),
          Style::default().fg(t.text_dim),
        )));
      }
    }

    let mut footer_lines: Vec<Line> = vec![
      Line::from(""),
      Line::from(Span::styled(
        "URL",
        Style::default().fg(t.header).add_modifier(Modifier::BOLD),
      )),
      Line::from(Span::styled(
        truncate(&item.url, inner.width as usize),
        Style::default().fg(t.text_dim),
      )),
    ];

    if let Some(notif) = &app.notification {
      if app.notification_item_id.as_deref() == Some(item.url.as_str()) {
        footer_lines.push(Line::from(""));
        footer_lines.push(Line::from(Span::styled(
          notif.as_str(),
          Style::default().fg(t.warning),
        )));
      }
    }

    if item.github_owner.is_some() && item.github_repo_name.is_some() {
      footer_lines.push(Line::from(""));
      footer_lines.push(Line::from(vec![
        Span::styled("Repo linked: press ", label_style),
        Span::styled("v", Style::default().fg(t.success)),
        Span::styled(" to view", label_style),
      ]));
    }

    let visible_height = inner.height as usize;
    let reserved_after_summary = footer_lines.len() + 2;
    let available_summary_lines = visible_height
      .saturating_sub(lines.len().saturating_add(reserved_after_summary))
      .min(7);
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
      "Summary",
      Style::default().fg(t.header).add_modifier(Modifier::BOLD),
    )));
    lines.extend(wrap_limited_ellipsis(
      &item.summary_short,
      detail_w,
      available_summary_lines,
      value_style,
    ));
    lines.extend(footer_lines);

    let para = Paragraph::new(lines);
    let t_para = std::time::Instant::now();
    frame.render_widget(para, inner);
    app.set_details_max_scroll(0);
    log::debug!(
      "details Paragraph render: {}ms",
      t_para.elapsed().as_millis()
    );
  } else {
    let hint = Paragraph::new("Select an item from the feed")
      .style(Style::default().fg(t.text_dim));
    frame.render_widget(hint, inner);
  }

  log::debug!(
    "draw_details_panel total: {}ms",
    t_details.elapsed().as_millis()
  );
}

fn filter_header(
  label: &'static str,
  t: &crate::theme::Theme,
) -> Line<'static> {
  Line::from(Span::styled(
    label,
    Style::default().fg(t.header).add_modifier(Modifier::BOLD),
  ))
}

fn filter_row(
  label: &'static str,
  active: bool,
  cursor: bool,
  t: &crate::theme::Theme,
) -> Line<'static> {
  let checkbox = if active { "[x]" } else { "[ ]" };
  if cursor {
    let hl = Style::default()
      .bg(t.bg_selection)
      .fg(t.text)
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
      Span::styled(checkbox, Style::default().fg(t.text)),
      Span::raw(" "),
      Span::styled(label, Style::default().fg(t.text)),
    ])
  } else {
    Line::from(vec![
      Span::raw("  "),
      Span::styled(checkbox, Style::default().fg(t.text_dim)),
      Span::raw(" "),
      Span::raw(label),
    ])
  }
}

fn push_activity_wrapped<'a>(
  lines: &mut Vec<Line<'a>>,
  label: &'static str,
  value: &str,
  label_style: Style,
  value_style: Style,
  value_width: usize,
  max_lines: usize,
) {
  let mut wrapped: Vec<String> = textwrap::wrap(value, value_width)
    .into_iter()
    .take(max_lines)
    .map(|line| line.into_owned())
    .collect();
  if wrapped.is_empty() {
    wrapped.push(String::new());
  }
  let label_text = format!("{label:<11}");
  for (idx, value_line) in wrapped.into_iter().enumerate() {
    let prefix = if idx == 0 { label_text.as_str() } else { "           " };
    lines.push(Line::from(vec![
      Span::styled(prefix.to_string(), label_style),
      Span::styled(value_line, value_style),
    ]));
  }
}

fn push_activity_continuation<'a>(
  lines: &mut Vec<Line<'a>>,
  value: &str,
  value_style: Style,
  value_width: usize,
  max_lines: usize,
) {
  for value_line in textwrap::wrap(value, value_width).into_iter().take(max_lines) {
    lines.push(Line::from(vec![
      Span::raw("           "),
      Span::styled(value_line.into_owned(), value_style),
    ]));
  }
}

fn push_detail_field<'a>(
  lines: &mut Vec<Line<'a>>,
  label: &'static str,
  value: &str,
  label_style: Style,
  value_style: Style,
  total_width: usize,
  max_lines: usize,
) {
  let label_w = 9;
  let value_w = total_width.saturating_sub(label_w).max(1);
  let label_text = format!("{label:<9}");
  let wrapped: Vec<String> = textwrap::wrap(value, value_w)
    .into_iter()
    .take(max_lines)
    .map(|line| line.into_owned())
    .collect();
  if wrapped.is_empty() {
    lines.push(Line::from(vec![
      Span::styled(label_text, label_style),
      Span::raw(""),
    ]));
    return;
  }
  for (idx, value_line) in wrapped.into_iter().enumerate() {
    let prefix = if idx == 0 { label_text.as_str() } else { "         " };
    lines.push(Line::from(vec![
      Span::styled(prefix.to_string(), label_style),
      Span::styled(value_line, value_style),
    ]));
  }
}

fn wrap_limited_ellipsis<'a>(
  value: &str,
  width: usize,
  max_lines: usize,
  style: Style,
) -> Vec<Line<'a>> {
  if max_lines == 0 {
    return Vec::new();
  }
  let width = width.max(1);
  let wrapped: Vec<String> =
    textwrap::wrap(value, width).into_iter().map(|line| line.into_owned()).collect();
  if wrapped.is_empty() {
    return vec![Line::from(Span::styled("", style))];
  }
  let overflowed = wrapped.len() > max_lines;
  let mut lines: Vec<String> = wrapped.into_iter().take(max_lines).collect();
  if overflowed {
    if let Some(last) = lines.last_mut() {
      let keep = width.saturating_sub(1);
      let trimmed = safe_truncate_chars(last.trim_end(), keep);
      *last = format!("{trimmed}…");
    }
  }
  lines.into_iter().map(|line| Line::from(Span::styled(line, style))).collect()
}

// ── Footer ─────────────────────────────────────────────────────────────────

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
  let t = app.active_theme.theme();
  let rows = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Length(1), Constraint::Length(1)])
    .split(area);

  let status_line: Option<Line> = if app.fulltext_loading {
    const SPINNER: &[&str] =
      &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let spin = SPINNER[app.spinner_frame % SPINNER.len()];
    Some(Line::from(Span::styled(
      format!("{spin} fetching article…"),
      Style::default().fg(t.warning),
    )))
  } else if let Some(msg) = &app.status_message {
    Some(Line::from(Span::styled(
      msg.clone(),
      Style::default().fg(t.warning),
    )))
  } else if app.is_loading {
    const SPINNER: &[&str] =
      &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let spin = SPINNER[app.spinner_frame % SPINNER.len()];
    let sources = app.loading_sources.join(", ");
    let prefix = if app.is_refreshing { "↻ refreshing" } else { "fetching" };
    Some(Line::from(Span::styled(
      format!("{spin} {prefix}: {}  │  {} items", sources, app.items.len()),
      Style::default().fg(t.warning),
    )))
  } else {
    None
  };

  let command_line = footer_command_line(app);
  if let Some(line) = status_line {
    frame.render_widget(Paragraph::new(vec![line]), rows[0]);
    frame.render_widget(Paragraph::new(vec![command_line]), rows[1]);
  } else {
    frame.render_widget(Paragraph::new(vec![command_line]), rows[0]);
    frame.render_widget(Paragraph::new(""), rows[1]);
  }
}

fn footer_command_line(app: &App) -> Line<'static> {
  let t = app.active_theme.theme();
  let ordinary = Style::default().fg(t.text_dim);
  let repo_style = Style::default().fg(t.success);
  let visible = app.visible_items().len();
  let total = app.items_for_tab().len();
  let filtered = !app.search_query.is_empty() || !app.active_filters.is_empty();
  let tab_target = if app.feed_tab == FeedTab::Discoveries {
    "inbox"
  } else {
    "discoveries"
  };
  let repo_available = !app.reader_active
    && !app.chat_fullscreen
    && app.focused_pane == PaneId::Feed
    && app
      .selected_item()
      .is_some_and(|item| item.github_owner.is_some() && item.github_repo_name.is_some());

  let mut spans = Vec::new();
  if filtered {
    spans.push(Span::styled(format!("{visible}/{total} filtered"), ordinary));
    spans.push(Span::styled(" | ", ordinary));
  }
  if repo_available {
    spans.push(Span::styled("v repo", repo_style));
    spans.push(Span::styled(" | ", ordinary));
  }
  spans.push(Span::styled(
    "w queue | r read | s skim | x archive | ",
    ordinary,
  ));
  spans.push(Span::styled(
    format!("ctrl+t leader | tab {tab_target} | ? help"),
    ordinary,
  ));

  Line::from(spans)
}

// ── A1: floating reader popup (Ldr+Enter) ─────────────────────────────────

fn draw_reader_popup(frame: &mut Frame, app: &mut App, area: Rect) {
  let t = app.active_theme.theme();
  let popup_w = (area.width as u32 * 70 / 100) as u16;
  let popup_h = ((area.height as u32 * 58 / 100) as u16).max(14);
  let popup_x = area.x + (area.width.saturating_sub(popup_w)) / 2;
  let popup_y = area.y + (area.height.saturating_sub(popup_h)) / 2;
  let popup_rect = Rect::new(popup_x, popup_y, popup_w, popup_h);

  frame.render_widget(Clear, popup_rect);

  let block = Block::default()
    .borders(Borders::ALL)
    .border_style(Style::default().fg(t.border_active))
    .title(Span::styled(
      " Reader · Esc close ",
      Style::default().fg(t.header).add_modifier(Modifier::BOLD),
    ));

  let block_inner = block.inner(popup_rect);
  let inner = Rect {
    x: block_inner.x.saturating_add(1),
    y: block_inner.y.saturating_add(1),
    width: block_inner.width.saturating_sub(2),
    height: block_inner.height.saturating_sub(2),
  };
  frame.render_widget(block, popup_rect);

  if let Some(editor) = app.reader_popup_editor.as_mut() {
    editor.update_layout(inner);
    cli_text_reader::draw_editor(frame, inner, editor);
  }
}

// ── A2 State 2: description popup over reader pane ────────────────────────

fn draw_abstract_popup(frame: &mut Frame, app: &App) {
  let t = app.active_theme.theme();
  let area = frame.area();
  let Some(item) = app.selected_item() else { return };

  let popup_w = (area.width * 70 / 100).max(52).min(area.width);
  let content_w = popup_w.saturating_sub(6) as usize;
  let title_wrapped: Vec<Line> = textwrap::wrap(&item.title, content_w)
    .into_iter()
    .take(3)
    .map(|s| {
      Line::styled(
        s.to_string(),
        Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
      )
    })
    .collect();

  let body_wrapped: Vec<Line> = if item.summary_short.is_empty() {
    vec![Line::styled(
      "No abstract available.",
      Style::default().fg(t.text_dim),
    )]
  } else {
    textwrap::wrap(&item.summary_short, content_w)
      .into_iter()
      .map(|s| Line::styled(s.to_string(), Style::default().fg(t.text)))
      .collect()
  };

  let desired_h = (title_wrapped.len() + body_wrapped.len() + 5)
    .clamp(9, area.height as usize);
  let max_h = area.height.saturating_sub(4).max(10).min(area.height);
  let popup_h = (desired_h as u16).min(max_h).max(9);
  let popup_x = area.x + (area.width.saturating_sub(popup_w)) / 2;
  let popup_y = area.y + (area.height.saturating_sub(popup_h)) / 2;
  let popup_rect = Rect::new(popup_x, popup_y, popup_w, popup_h);

  frame.render_widget(Clear, popup_rect);

  let block = Block::default()
    .borders(Borders::ALL)
    .border_style(Style::default().fg(t.border_active))
    .title(Span::styled(
      " Abstract · Space/Esc close ",
      Style::default().fg(t.header).add_modifier(Modifier::BOLD),
    ));

  let block_inner = block.inner(popup_rect);
  let inner = Rect {
    x: block_inner.x.saturating_add(1),
    y: block_inner.y.saturating_add(1),
    width: block_inner.width.saturating_sub(2),
    height: block_inner.height.saturating_sub(2),
  };
  frame.render_widget(block, popup_rect);

  if inner.height == 0 {
    return;
  }

  let mut lines: Vec<Line> = Vec::new();
  lines.extend(title_wrapped);
  lines.push(Line::raw(""));
  lines.extend(body_wrapped);

  let para = Paragraph::new(lines).wrap(Wrap { trim: false });
  frame.render_widget(para, inner);
}

fn draw_narrow_feed_details_popup(frame: &mut Frame, app: &App, area: Rect) {
  let t = app.active_theme.theme();
  let popup_h = (area.height * 40 / 100).max(6).min(area.height);
  let popup_w = area.width.saturating_sub(4).max(20);
  let popup_x = area.x + (area.width.saturating_sub(popup_w)) / 2;
  let popup_y = area.y + area.height.saturating_sub(popup_h);
  let popup_rect = Rect::new(popup_x, popup_y, popup_w, popup_h);

  frame.render_widget(Clear, popup_rect);

  let block = Block::default()
    .borders(Borders::ALL)
    .border_style(Style::default().fg(t.border_active))
    .title(Span::styled(
      " Details · j/k: scroll  d/Esc: close ",
      Style::default().fg(t.accent),
    ));

  let inner = block.inner(popup_rect);
  frame.render_widget(block, popup_rect);

  if inner.height == 0 {
    return;
  }

  let items = app.items_for_tab();
  let sel = app.active_selected_index();
  let Some(item) = items.get(sel) else { return };

  let text = format!("{}\n\n{}", item.title, item.summary_short);
  let scroll = app.details_scroll;
  let para = Paragraph::new(text)
    .wrap(ratatui::widgets::Wrap { trim: false })
    .scroll((scroll as u16, 0))
    .style(Style::default().fg(t.text));
  frame.render_widget(para, inner);
}

// ── A2 State 3: persistent bottom pane (feed list or details) ─────────────

fn draw_reader_bottom_pane(frame: &mut Frame, app: &App, area: Rect) {
  let t = app.active_theme.theme();
  const POPUP_H: u16 = 11; // border(2) + hint row(1) + sep(1) + content(7)
  let popup_w = (area.width as u32 * 60 / 100) as u16;
  let popup_x = area.x + (area.width.saturating_sub(popup_w)) / 2;
  let popup_y = area.y + area.height.saturating_sub(POPUP_H);
  let popup_rect = Rect::new(popup_x, popup_y, popup_w, POPUP_H);

  frame.render_widget(Clear, popup_rect);

  let focused = app.reader_bottom_focused;
  let border_color = if focused { t.border_active } else { t.border };

  let title_str = if app.reader_bottom_details {
    " Details · d: back  j/k: scroll  Esc: back "
  } else {
    " Feed · j/k: navigate  Enter: open  d: details  q: close "
  };
  let block = Block::default()
    .borders(Borders::ALL)
    .border_style(Style::default().fg(border_color))
    .title(Span::styled(title_str, Style::default().fg(t.accent)));

  let inner = block.inner(popup_rect);
  frame.render_widget(block, popup_rect);

  if inner.height == 0 {
    return;
  }

  if app.reader_bottom_details {
    draw_bottom_pane_details(frame, app, inner);
  } else {
    draw_bottom_pane_feed(frame, app, inner);
  }
}

fn draw_bottom_pane_details(frame: &mut Frame, app: &App, area: Rect) {
  let t = app.active_theme.theme();
  let sel = app.reader_feed_popup_selected;
  let items = app.items_for_tab();
  let Some(item) = items.get(sel) else { return };

  let scroll = app.reader_bottom_scroll;
  let text = format!("{}\n\n{}", item.title, item.summary_short);
  let para = Paragraph::new(text)
    .wrap(ratatui::widgets::Wrap { trim: false })
    .scroll((scroll as u16, 0))
    .style(Style::default().fg(t.text_dim));
  frame.render_widget(para, area);
}

fn draw_bottom_pane_feed(frame: &mut Frame, app: &App, area: Rect) {
  let t = app.active_theme.theme();
  let items = app.items_for_tab();
  let sel = app.reader_feed_popup_selected;
  let max_visible = area.height as usize;

  // Auto-scroll offset to keep selection visible.
  let offset = if sel >= max_visible { sel - max_visible + 1 } else { 0 };

  let mut lines: Vec<Line> = Vec::new();
  for (i, item) in items.iter().enumerate().skip(offset).take(max_visible) {
    let is_selected = i == sel;
    let title_w = area.width.saturating_sub(2) as usize;
    let title = truncate_str(&item.title, title_w);
    if is_selected {
      lines.push(Line::from(Span::styled(
        format!("▶ {title}"),
        Style::default().fg(t.accent).add_modifier(Modifier::BOLD),
      )));
    } else {
      lines.push(Line::from(Span::styled(
        format!("  {title}"),
        Style::default().fg(t.text),
      )));
    }
  }

  frame.render_widget(Paragraph::new(lines), area);
}

fn truncate_str(s: &str, max: usize) -> String {
  let chars: Vec<char> = s.chars().collect();
  if chars.len() <= max {
    s.to_string()
  } else {
    chars[..max.saturating_sub(1)].iter().collect::<String>() + "…"
  }
}

// ── Sources popup ──────────────────────────────────────────────────────────

fn draw_sources_popup(frame: &mut Frame, app: &App) {
  let t = app.active_theme.theme();
  let area = frame.area();
  let popup_w =
    (area.width as u32 * 58 / 100).max(72).min(area.width as u32) as u16;
  let content_rows = 6
    + app.sources_popup_arxiv_cats().len() as u16
    + crate::config::PREDEFINED_SOURCES.len() as u16
    + app.config.sources.custom_feeds.len().max(1) as u16;
  let popup_h = content_rows
    .saturating_add(7)
    .max(20)
    .min((area.height as u32 * 70 / 100) as u16)
    .min(area.height);
  let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
  let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
  let popup_area = ratatui::layout::Rect::new(x, y, popup_w, popup_h);

  frame.render_widget(Clear, popup_area);

  let block = Block::default()
    .borders(Borders::ALL)
    .title(Span::styled(
      " Manage Sources ",
      Style::default().fg(t.text).add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(t.border))
    .style(Style::default().bg(t.bg_panel));

  let inner = block.inner(popup_area);
  frame.render_widget(block, popup_area);

  let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Min(0), Constraint::Length(2)])
    .split(inner);
  let content_area = chunks[0];
  let footer_area = chunks[1];

  let w = content_area.width as usize;
  let hrule = "─".repeat(w.saturating_sub(4));
  let cats = app.sources_popup_arxiv_cats();
  let cats_count = cats.len();
  let sources_count = crate::config::PREDEFINED_SOURCES.len();
  let custom_feeds = &app.config.sources.custom_feeds;
  let cursor = app.sources_cursor;

  let gray = Style::default().fg(t.text_dim);
  let white = Style::default().fg(t.text);
  let bold_white = Style::default().fg(t.header).add_modifier(Modifier::BOLD);
  let cyan = Style::default().fg(t.accent);
  let selected_style =
    Style::default().bg(t.bg_selection).fg(t.text).add_modifier(Modifier::BOLD);

  let mut lines: Vec<Line> = Vec::new();

  lines.push(Line::from(Span::styled("  Add source", bold_white)));

  let input_active = app.sources_input_active;
  let input_focused = cursor == 0;
  let input_display = if app.sources_input.is_empty() && !input_active {
    "paste a URL...".to_string()
  } else if input_active {
    format!("{}_", app.sources_input)
  } else {
    app.sources_input.clone()
  };
  lines.push(Line::from(vec![
    Span::styled(
      if input_focused { "  > " } else { "    " },
      if input_active {
        Style::default().fg(t.success)
      } else if input_focused {
        Style::default().fg(t.accent)
      } else {
        gray
      },
    ),
    Span::styled(
      truncate(&input_display, w.saturating_sub(8)),
      if input_active || input_focused {
        Style::default().fg(t.text)
      } else {
        gray
      },
    ),
  ]));

  let detect_line = match &app.sources_detect_state {
    SourcesDetectState::Idle => {
      if input_focused && !app.sources_input.is_empty() && !input_active {
        Line::from(Span::styled("  Press Enter to detect feed type", gray))
      } else {
        Line::from("")
      }
    }
    SourcesDetectState::Detecting => {
      Line::from(Span::styled("  Detecting...", Style::default().fg(t.warning)))
    }
    SourcesDetectState::Result(r) => match r {
      DiscoverResult::ArxivCategory(code) => Line::from(Span::styled(
        format!("  Detected: arXiv category {code} — press Enter to confirm"),
        Style::default().fg(t.success),
      )),
      DiscoverResult::HuggingFaceAlreadyEnabled => Line::from(Span::styled(
        "  Detected: HuggingFace daily papers — already enabled",
        gray,
      )),
      DiscoverResult::RssFeed { url, .. } => {
        let display = truncate(url, w.saturating_sub(36));
        Line::from(Span::styled(
          format!("  Detected: RSS feed at {display} — press Enter to confirm"),
          Style::default().fg(t.success),
        ))
      }
      DiscoverResult::Failed(msg) => Line::from(Span::styled(
        format!("  {msg}"),
        Style::default().fg(t.error),
      )),
    },
  };
  lines.push(detect_line);

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

  let para = Paragraph::new(lines).style(Style::default().bg(t.bg_panel));
  frame.render_widget(para, content_area);

  let footer_rule = "─".repeat(footer_area.width as usize);
  let footer = Paragraph::new(vec![
    Line::from(Span::styled(footer_rule, Style::default().fg(t.border))),
    Line::from(Span::styled(
      "  j/k navigate · space toggle · enter add source · d delete · esc back",
      white,
    )),
  ])
  .style(Style::default().bg(t.bg_panel));
  frame.render_widget(footer, footer_area);
}

// ── Settings view ──────────────────────────────────────────────────────────

fn draw_settings(frame: &mut Frame, app: &App) {
  let t = app.active_theme.theme();
  let area = frame.area();

  let block = Block::default()
    .borders(Borders::ALL)
    .title(Span::styled(
      " Settings ",
      Style::default().fg(t.text).add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(t.border));

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
      if editing {
        t.success
      } else {
        t.accent
      }
    } else {
      t.border
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

  let gray = Style::default().fg(t.text_dim);
  let white = Style::default().fg(t.text);
  let bold_white = Style::default().fg(t.header).add_modifier(Modifier::BOLD);

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
          Style::default().fg(t.accent)
        } else {
          gray
        },
      ),
      Span::styled(
        app.settings_default_chat_provider.clone(),
        if app.settings_field == 4 {
          Style::default().fg(t.success).add_modifier(Modifier::BOLD)
        } else {
          white
        },
      ),
    ]),
    Line::from(""),
    Line::from(Span::styled("  Theme", white)),
    Line::from(vec![
      Span::styled(
        "  [enter to choose] ",
        if app.settings_field == 5 {
          Style::default().fg(t.accent)
        } else {
          gray
        },
      ),
      Span::styled(
        app.active_theme.info().name.to_string(),
        if app.settings_field == 5 {
          Style::default().fg(t.success).add_modifier(Modifier::BOLD)
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
      Style::default().fg(t.success),
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

fn draw_theme_picker(frame: &mut Frame, app: &App) {
  let t = app.active_theme.theme();
  let area = frame.area();
  let popup_w =
    (area.width as u32 * 56 / 100).max(44).min(area.width as u32) as u16;
  let popup_h =
    (area.height as u32 * 62 / 100).max(16).min(area.height as u32) as u16;
  let x = area.x + area.width.saturating_sub(popup_w) / 2;
  let y = area.y + area.height.saturating_sub(popup_h) / 2;
  let popup = Rect::new(x, y, popup_w, popup_h);

  frame.render_widget(Clear, popup);
  let block = Block::default()
    .borders(Borders::ALL)
    .title(Span::styled(
      " Theme ",
      Style::default().fg(t.text).add_modifier(Modifier::BOLD),
    ))
    .border_style(Style::default().fg(t.border))
    .style(Style::default().bg(t.bg_popup));
  let inner = block.inner(popup);
  frame.render_widget(block, popup);

  if inner.height == 0 || inner.width == 0 {
    return;
  }

  let footer_h = 1u16.min(inner.height);
  let list_h = inner.height.saturating_sub(footer_h);
  let list_area = Rect { height: list_h, ..inner };
  let footer_area = Rect { y: inner.y + list_h, height: footer_h, ..inner };

  let all = ui_theme::ThemeId::all();
  let scroll = app.theme_picker_scroll.min(all.len().saturating_sub(1));
  let end = (scroll + list_h as usize).min(all.len());
  let mut lines: Vec<Line> = Vec::new();
  let mut last_group: Option<ui_theme::ThemeGroup> =
    if scroll == 0 { None } else { Some(all[scroll - 1].info().group) };

  for (idx, id) in
    all.iter().enumerate().skip(scroll).take(end.saturating_sub(scroll))
  {
    let info = id.info();
    if last_group != Some(info.group) {
      lines.push(Line::from(Span::styled(
        format!("  {}", info.group.label()),
        Style::default().fg(t.header).add_modifier(Modifier::BOLD),
      )));
      last_group = Some(info.group);
      if lines.len() >= list_h as usize {
        break;
      }
    }

    let theme = id.theme();
    let selected = idx == app.theme_picker_cursor;
    let row_style = if selected {
      Style::default()
        .fg(t.text)
        .bg(t.bg_selection)
        .add_modifier(Modifier::BOLD)
    } else {
      Style::default().fg(t.text)
    };
    let marker = if selected { ">" } else { " " };
    lines.push(Line::from(vec![
      Span::styled(format!(" {marker} "), row_style),
      Span::styled(format!("{:<20}", info.name), row_style),
      swatch(theme.accent),
      swatch(theme.header),
      swatch(theme.text_dim),
      swatch(theme.bg_selection),
      swatch(theme.success),
      swatch(theme.warning),
      swatch(theme.error),
      Span::styled(format!("  {}", info.id), Style::default().fg(t.text_dim)),
    ]));

    if lines.len() >= list_h as usize {
      break;
    }
  }

  frame.render_widget(
    Paragraph::new(lines).style(Style::default().bg(t.bg_popup)),
    list_area,
  );

  frame.render_widget(
    Paragraph::new(Span::styled(
      " j/k: preview  enter: select  esc: cancel",
      Style::default().fg(t.text_dim).bg(t.bg_popup),
    )),
    footer_area,
  );
}

fn swatch(color: Color) -> Span<'static> {
  Span::styled("  ", Style::default().bg(color))
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
/// screen rows, including one spacer row between feed items.
fn count_visible_items(
  items: &[&crate::models::FeedItem],
  list_offset: usize,
  viewport_rows: usize,
  title_wrap_w: usize,
) -> usize {
  let mut rows_used = 0usize;
  let mut count = 0usize;
  for item in items.iter().skip(list_offset) {
    let item_height = if item.title.len() > title_wrap_w { 3 } else { 2 };
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
  t: &crate::theme::Theme,
) -> (Rect, Rect) {
  let s = Style::default().fg(t.border);

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
  t: &crate::theme::Theme,
) -> (Rect, Rect) {
  let s = Style::default().fg(t.border);

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
      ("j / k", "Move down / up"),
      ("g / G", "Jump to top / bottom"),
      ("l / →", "Focus details pane"),
      ("Tab", "Switch Inbox / Discoveries"),
      ("f", "Focus filter panel"),
      ("Enter", "Open paper in reader"),
      ("Esc", "Quit (from feed) / go back"),
      ("click", "Focus any pane"),
    ],
  ),
  (
    "Leader",
    &[
      ("Ldr = Ctrl+T", ""),
      ("Ldr+?", "This help screen"),
      ("Ldr+q", "Quit application"),
      ("Ldr+Enter", "Open paper in floating popup"),
      ("Ldr+v", "Cycle reader layout (full→split→dual)"),
      ("Ldr+Esc", "Step back reader state"),
      ("Ldr+s", "Open sources manager"),
      ("Ldr+n", "Toggle notes panel"),
      ("Ldr+c", "Toggle chat panel"),
      ("Ldr+z", "Move chat top / bottom"),
      ("Ldr+S", "Open settings"),
      ("Ldr+h / Ldr+l", "Focus pane left / right"),
      ("Ldr+j / Ldr+k", "Focus pane down / up"),
      ("Ldr+1 / 2 / 3", "Focus pane by number"),
      ("", ""),
      ("Notes / Reader tabs", ""),
      ("Ldr+[", "Previous tab"),
      ("Ldr+]", "Next tab"),
      ("Ldr+w", "Close current tab"),
    ],
  ),
  (
    "Feed",
    &[
      ("/", "Search"),
      ("Esc", "Clear search"),
      ("R", "Refresh all sources"),
      ("o", "Open URL in browser"),
      ("v", "Open repo viewer"),
      ("i", "Mark Inbox"),
      ("s", "Mark Skimmed"),
      ("r", "Mark DeepRead"),
      ("w", "Mark Queued"),
      ("x", "Archive"),
      ("", ""),
      ("Filter panel", ""),
      ("Space", "Toggle filter at cursor"),
      ("c", "Clear all filters"),
      ("Tab / f", "Return to feed"),
    ],
  ),
  (
    "Reader",
    &[
      ("vim keys", "Standard vim navigation"),
      ("Tab", "Switch primary / secondary pane"),
      ("Ldr+v", "Toggle bottom feed panel (dual mode)"),
      ("Ldr+n", "Toggle notes panel"),
      ("q / Esc", "Close / step back reader state"),
      ("", ""),
      ("Tabs", ""),
      ("Ldr+t", "Open in new tab (prompt if dual)"),
      ("Ldr+[", "Previous tab"),
      ("Ldr+]", "Next tab"),
      ("Ldr+w", "Close current tab"),
      ("", ""),
      ("Voice", ""),
      ("r", "Read current paragraph"),
      ("R", "Read from cursor to end"),
      ("Ctrl+p", "Continuous reading (auto-advance)"),
      ("Space", "Pause / resume playback"),
      ("c", "Re-centre on playing paragraph"),
      ("Esc", "Stop playback"),
    ],
  ),
  (
    "Discoveries",
    &[
      ("Tab", "Switch Inbox / Discoveries"),
      ("/", "Open AI topic search"),
      ("", ""),
      ("Plan checklist", ""),
      ("j / k", "Navigate results"),
      ("Space", "Toggle source in plan"),
      ("a", "Add all selected sources"),
      ("Esc", "Clear discovery plan"),
    ],
  ),
  (
    "Chat",
    &[
      ("Enter", "Send message"),
      ("j / k", "Scroll chat history"),
      ("Esc", "Back to session list"),
      ("Ldr+c", "Close chat panel"),
      ("Ldr+z", "Move chat top / bottom"),
    ],
  ),
  (
    "Settings",
    &[
      ("Ldr+S", "Open settings"),
      ("j / k", "Navigate fields"),
      ("Enter", "Edit field or cycle option"),
      ("s / S", "Save all fields"),
      ("p", "Manage sources"),
      ("q / Esc", "Close settings"),
      ("", ""),
      ("Sources panel", ""),
      ("Space", "Toggle source on / off"),
      ("Enter / /", "Add custom source (URL)"),
      ("d", "Delete custom feed"),
      ("Esc", "Back to settings"),
    ],
  ),
  (
    "Repo Viewer",
    &[
      ("j / k", "Navigate file tree"),
      ("Enter", "Open file or folder"),
      ("b / Backspace", "Go back"),
      ("Tab", "Switch tree / content pane"),
      ("h / l", "Scroll content left / right"),
      ("+/= / -", "Zoom in / out"),
      ("y", "Copy file path"),
      ("d", "Download file"),
      ("q", "Close viewer"),
    ],
  ),
];

pub const HELP_SECTION_COUNT: usize = HELP_SECTIONS.len();

fn draw_help_overlay(frame: &mut Frame, app: &mut App) {
  let t = app.active_theme.theme();
  let area = frame.area();

  let (_, bindings) =
    HELP_SECTIONS[app.help_section.min(HELP_SECTIONS.len() - 1)];
  let body_rows = bindings.len() as u16 + 3;
  let popup_w = ((area.width as f32 * 0.68) as u16).max(64).min(area.width);
  let popup_h = body_rows
    .saturating_add(5)
    .max(14)
    .min((area.height as f32 * 0.72) as u16)
    .min(area.height);
  let popup_x = (area.width.saturating_sub(popup_w)) / 2;
  let popup_y = (area.height.saturating_sub(popup_h)) / 2;
  let popup_rect = Rect::new(popup_x, popup_y, popup_w, popup_h);

  frame.render_widget(Clear, popup_rect);

  let block = Block::default()
    .borders(Borders::ALL)
    .border_style(Style::default().fg(t.border_active))
    .title(Span::styled(
      " Help · q/Esc close ",
      Style::default().fg(t.header).add_modifier(Modifier::BOLD),
    ));
  let block_inner = block.inner(popup_rect);
  let inner = Rect {
    x: block_inner.x.saturating_add(1),
    y: block_inner.y,
    width: block_inner.width.saturating_sub(2),
    height: block_inner.height,
  };
  frame.render_widget(block, popup_rect);

  let layout_rows = Layout::vertical([
    Constraint::Length(1),
    Constraint::Length(1),
    Constraint::Min(0),
    Constraint::Length(1),
  ])
  .split(inner);
  let tab_area = layout_rows[0];
  let sep_area = layout_rows[1];
  let body_area = layout_rows[2];
  let footer_area = layout_rows[3];

  let tab_style_active =
    Style::default().fg(t.text).add_modifier(Modifier::BOLD);
  let tab_style_inactive = Style::default().fg(t.text_dim);
  let mut tab_spans: Vec<Span> = Vec::new();
  for (i, (name, _)) in HELP_SECTIONS.iter().enumerate() {
    let label = format!("{name}");
    if i == app.help_section {
      tab_spans.push(Span::styled(label, tab_style_active));
    } else {
      tab_spans.push(Span::styled(label, tab_style_inactive));
    }
    if i + 1 < HELP_SECTIONS.len() {
      tab_spans.push(Span::styled("  |  ", Style::default().fg(t.border)));
    }
  }
  frame.render_widget(Paragraph::new(Line::from(tab_spans)), tab_area);

  let sep = "─".repeat(sep_area.width as usize);
  frame.render_widget(
    Paragraph::new(Span::styled(sep, Style::default().fg(t.border))),
    sep_area,
  );

  let key_col_w = 18u16;

  let key_style = Style::default().fg(t.accent);
  let header_style = Style::default().fg(t.header).add_modifier(Modifier::BOLD);
  let desc_style = Style::default().fg(t.text);
  let gray = Style::default().fg(t.text_dim);

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

  let total_lines = body_lines.len() as u16;
  let max_scroll = total_lines.saturating_sub(body_area.height);
  app.help_scroll = app.help_scroll.min(max_scroll);
  let scroll = app.help_scroll;

  frame
    .render_widget(Paragraph::new(body_lines).scroll((scroll, 0)), body_area);
  frame.render_widget(
    Paragraph::new(Line::from(Span::styled(
      "Tab/h/l next section | j/k scroll | q/Esc close",
      gray,
    ))),
    footer_area,
  );
}
