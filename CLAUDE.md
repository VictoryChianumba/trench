# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

### /checkpoint
Saves current progress with a commit message.
Usage: /checkpoint "description of what was done"
Command: git add -A && git commit -m "$1"

### Nightly checkpoint reminder
Around 10pm each day, Claude should ask the user: "Should we commit the current version to GitHub?"
Before asking, draft a commit message summarising the session's changes and present it for review.
Only commit after the user has read and approved the draft message.

```sh
# Build and run (primary development workflow)
cargo run -p hygg-reader --release -- test-data/pdf/pdfreference1.7old.pdf

# Build all crates
cargo build --release

# Run tests
cargo test

# Run tests for a specific crate
cargo test -p cli-text-reader

# Run a single test
cargo test -p cli-text-reader test_name

# Check formatting
cargo fmt --check

# Lint
cargo clippy

# Build trench separately
cargo build -p trench --release
```

Rust edition: 2024, MSRV: 1.88. The `ci.sh` script uses the nightly toolchain for `cargo fix`, `cargo udeps`, and `cargo audit`.

## Workspace Structure

```
hygg-reader/       → main reader binary: arg parsing, doc conversion → cli-text-reader
cli-text-reader/   → the TUI reader (all editor logic lives here)
cli-pdf-to-text/   → PDF → plain text conversion
cli-epub-to-text/  → EPUB → plain text conversion
cli-justify/       → text justification/wrapping
hygg-shared/       → shared utilities
redirect-stderr/   → stderr redirection helper
trench/            → separate binary: AI research feed aggregator TUI
```

**hygg-reader pipeline**: arg parsing → OCR (optional, via `ocrmypdf`) → format conversion (PDF/EPUB/pandoc) → `cli-justify::justify()` → `cli-text-reader::run_cli_text_reader()`.

## Workspace-wide Clippy Allowances

`needless_return`, `unused_imports`, `implicit_saturating_sub`, `single_component_path_imports` are allowed workspace-wide.

## cli-text-reader Architecture

This crate is the core. Everything is implemented as `impl Editor` blocks spread across many files. The `Editor` struct is defined in `src/core_state.rs` and re-exported via `src/editor/core.rs`.

**Main loop** (`src/editor/display_loop.rs`): polls voice status, handles crossterm events, triggers redraws. Uses `needs_redraw` flag — call `self.mark_dirty()` to request a redraw.

**Event routing** (`src/editor/event_handler.rs` → `src/editor/normal_mode.rs`): `handle_event` dispatches to mode-specific handlers. Normal mode calls handlers in priority order: tmux prefix → voice keys → control keys → operator pending → search/visual → navigation.

**Modes** (`src/core_types.rs`): `EditorMode` — Normal, VisualChar, VisualLine, Search, ReverseSearch, Command, CommandExecution, Tutorial. Mode is stored per-buffer in `BufferState`; use `get_active_mode()` / `set_active_mode()`.

**Voice/TTS** (`src/voice/`, `src/editor/voice_control.rs`):
- `PlaybackController` owns a background thread (`playback_loop`) that receives `PlaybackCommand` over an mpsc channel and drives rodio audio playback.
- Text is split into ≤4500-char chunks via `chunk_paragraphs()` in `src/voice/mod.rs`.
- `VoicePlayingInfo` (shared via `Arc<Mutex>`) tracks which doc lines are playing and timing for word-highlight animation.
- `sync_voice_status()` is called each tick in the display loop — this is the hook point for detecting playback completion.
- TTS uses ElevenLabs API. Config (`ELEVENLABS_API_KEY`, `VOICE_ID`, `PLAYBACK_SPEED`) lives in `~/.config/hygg-reader/.env`.

**Config** (`src/config.rs`): loaded from `~/.config/hygg-reader/.env` via `dotenvy`. Call `load_config()` at startup; `save_config()` persists changes.

**Persistence**: Progress saved per-document using a hash of the document content (`src/progress.rs`). Bookmarks and highlights also keyed by document hash (`src/bookmarks.rs`, `src/highlights.rs`). All files live under `~/.config/hygg-reader/`.

**Buffers**: The editor supports multiple `BufferState` buffers (used for split-view command output). Buffer 0 is always the document. Active buffer accessed via `self.active_buffer` index.

**Display**: `draw_content_buffered` renders to a `Vec<u8>` then flushes in one write to minimize flicker. Status line rendered separately by `draw_status_line` / `draw_status_line_buffered`.

**Key conventions**:
- All editor methods are `impl Editor` — no separate structs for subsystems.
- Handler functions return `Result<Option<bool>, ...>`: `Some(true)` = quit, `Some(false)` = handled (stop propagation), `None` = not handled (continue to next handler).
- `self.offset` = first visible line index; `self.cursor_y` = cursor row on screen; `self.offset + self.cursor_y` = current doc line.

## trench Architecture

A separate TUI binary (`trench/src/main.rs`) that aggregates AI research feeds. No async — uses `std::sync::mpsc` and `reqwest::blocking` throughout.

### Data model (`src/models/`)

`FeedItem` is the central type: `id`, `title`, `source_platform`, `content_type`, `domain_tags`, `signal` (Primary/Secondary/Tertiary), `published_at`, `authors`, `summary_short`, `workflow_state` (Inbox/Skimmed/Queued/DeepRead/Archived), `url`, `upvote_count`. `upvote_count` has `#[serde(default)]` for cache backward-compatibility.

`FeedItem::compute_signal()` derives signal from platform and upvote count. `map_arxiv_category()` and `detect_subtopics()` live in `src/models/categories.rs`.

### Ingestion pipeline (`src/ingestion/`)

Background thread in `main.rs` fetches all sources sequentially then runs enrichment:

1. `arxiv::fetch()` — arXiv Atom API (cs.LG + cs.AI + stat.ML). Maps category codes via `map_arxiv_category()`, detects subtopics via `detect_subtopics()`.
2. `huggingface::fetch()` — Scrapes HF daily papers page (two-pass: h3 for titles, entity-encoded JSON for upvotes/authors), then makes one batched arXiv API call to fill `summary_short` for all items.
3. `rss::fetch()` — Generic RSS 2.0 / Atom parser for OpenAI blog, DeepMind blog, Import AI, The Batch. Handles CDATA via `Event::CData`. Anthropic has no RSS feed and is intentionally skipped.
4. `semantic_scholar::enrich()` — Enriches arXiv items with citation counts and fields of study. 7-day TTL cache at `~/.config/trench/enrichment_cache.json`. Entries with empty `fields_of_study` are invalidated on load.

Each source sends `FetchMessage::Items(Vec<FeedItem>)` + `FetchMessage::SourceComplete(name)` over mpsc. After all sources, sends enriched batch + `AllComplete`.

### App state and merge logic (`src/app.rs`)

`App::process_incoming()` drains the channel each frame (non-blocking `try_recv` loop):
- **URL dedup**: overwrites cached item with fresh data; workflow state comes from `persisted_states` (keyed by URL).
- **ArXiv ID dedup**: collapses HF and arXiv entries for the same paper — arXiv entry wins. The HF entry's `workflow_state` is preserved onto the arXiv entry when replacing.

Items are sorted by `published_at` descending after each batch. Cache is written to `~/.config/trench/cache.json` immediately.

### Store (`src/store/`)

- `store::load()` / `store::save()` — workflow states, keyed by URL, at `~/.config/trench/state.json`.
- `store::cache` — full `Vec<FeedItem>` cache, loaded at startup so the TUI is populated before network fetches complete.
- `store::enrichment_cache` — Semantic Scholar results, 7-day TTL via Julian Day Number arithmetic (no chrono).

### UI (`src/ui/layout.rs`)

Single `draw(frame, app)` entry point. Feed view: tab bar → search row → item table + details panel → status bar with braille spinner during loading. Reader view: full-screen content with header bar. Details panel shows upvote count for HuggingFace items.

---

# Tentative — TODO Checklist

Status markers: [x] done  [-] in progress / partial  [ ] not started

## Bugs to fix
- [ ] Voice mode broken in hygg rewrite (fix after ElevenLabs credits topped up)
- [-] Chat scrolling not smooth — key repeat and trackpad inertia (partially fixed)
- [x] Raw ANSI escape codes leaking into right pane in reader mode (strip_ansi fixed: CSI + OSC sequences now fully stripped)
- [x] Notes opening on vim `n` keypress — fixed, leader key now required

## Leader key (Ctrl+T) — app-wide
- [x] Implement Ctrl+T as global leader key for all Tentative keybindings
- [x] Hygg vim keybindings remain unchanged (no leader needed)
- [x] Update footer to always show `Ldr: ctrl+t` and `Ldr+[key]` for all bindings
- [x] Full keybinding descriptions reserved for help screen only

## Chat panel redesign
- [x] Move chat from right pane to bottom panel (Feynman-style)
- [x] Chat panel height fixed, not additive
- [x] Chat streams below main panes, scrollable within its window
- [x] `Ldr+z` moves chat panel to top / bottom
- [x] Subtle background color difference separates chat from main panes
- [-] Clean minimal message style — no heavy borders on messages
- [-] User messages plain, assistant messages slightly dimmed/indented

## Hygg integration
- [x] Step 10: Wire rewritten cli-text-reader into Tentative reader pane
- [x] Reader mode: full width when no right panel active
- [x] Reader mode: 60/40 split when notes active
- [ ] Voice mode: fix wiring after ElevenLabs credits topped up
- [x] Floating hygg reader popup — Ldr+Enter, dismissible with Esc
- [x] Secondary split view — three-state cycle (full → split → dual) via Ldr+v

## Reader pane tabs
- [ ] Tabbed reader panes: open multiple papers and switch between them (like browser tabs)
- [ ] Tab bar across the top of the reader area showing open paper titles (truncated)
- [ ] Ldr+t to open a new tab; Ldr+w to close current tab
- [ ] Switch tabs with Ldr+[ / Ldr+] or number keys
- [ ] Side-by-side mode: two papers simultaneously (distinct from current dual-reader)
- [ ] Tab state persists across sessions

## Dashboard / Home view
- [x] Add a home/dashboard screen as the default landing view — shown in wider details pane until first navigation; sections: Continue Reading, Your Queue, At a Glance (counts), Recent (last 48h), footer (provider + sources)
- [x] Persist last_read across sessions (store/ui.json; load on startup, save on quit)
- [ ] Recent research topic tracking (deferred — needs design)
- [ ] Show active AI model in use
- [ ] Show last read paper (title, source, position) with quick-resume action
- [ ] Show current research topic / focus area (derived from recent feed activity or manually set)
- [ ] Design question: separate tab/view, or overlay on the feed list?

## Notes
- [x] Notes accessible from reader mode via Ldr+n
- [x] Notes panel opens in right pane alongside reader (60/40 split)
- [x] Notes anchored to paper (keyed by URL/arXiv ID)
- [x] Notes persist to disk
- [ ] Tabbed notes: multiple note documents open at once, tab bar across top of notes pane
- [ ] Reopening a paper restores its notes tab

## Help screen
- [x] Design and implement full help screen
- [x] All keybindings documented including leader key notation
- [x] Accessible via Ldr+?
- [x] Repo viewer, Discoveries, filter panel, settings all covered

## Source discovery (agent-based)
- [-] Agent-assisted source discovery — discovery/ module exists, UI partially wired
- [ ] Trending/popularity filter using existing signals + web search
- [x] Default sources remain, agents are supplementary

## UI polish
- [x] Themes system — Dark / Light / AMOLED, runtime switching
- [x] Settings screen: theme selection via cycle field
- [-] Overall UI improvements (ongoing)

## Settings popup (hygg-reader)
- [x] :settings / :config / :set command opens popup in reader
- [x] TTS_PROVIDER exposed as cycle-select (auto → elevenlabs → say → piper)
- [x] SAY_VOICE field exposed with hint
- [x] Per-field hints shown when field is focused
- [x] TTS_PROVIDER and SAY_VOICE persist via save_config

## Semantic Scholar
- [-] Rate limiting partially handled (caps applied, no key)
- [ ] Apply for proper API key
- [ ] Re-enable full enrichment once key obtained

## Hygg rewrite (parallel agents)
- [x] CC plan: stages 6-9 complete on hygg-rewrite-codex branch
- [x] Codex plan: stages 1-5 complete on same branch
- [x] Wire winning approach into Tentative (Step 10 complete)
- [ ] Compare and clean up both agent branches

## block-reader / arXiv paper reading (feat/block-model branch)

### Parse quality
- [x] doc-model crate: Block and VisualLine types
- [x] arxiv-render crate: LaTeX → Block pipeline (to_blocks)
- [x] block-reader crate: TUI reader with ratatui
- [x] `\newcommand` macro extraction and expansion in prose and math
- [x] Theorem / lemma / proof environments → numbered header + ∎ end marker
- [x] `\begin{enumerate}` → numbered markers; `\begin{itemize}` → bullets
- [x] Figure / table captions extracted → `[Figure: caption text]`
- [x] `\begin{tabular}` parsed → Matrix blocks with ┌/│/└ border art
- [x] Footnotes deferred to "Notes" section at end of document
- [x] Accent characters: `\'e`→é, `\"o`→ö, `\^a`→â, and alphabetic forms
- [x] Special letter commands: `\ss`→ß, `\ae`→æ, `\o`→ø, etc.
- [x] `~`→space, `---`→em dash, `--`→en dash
- [x] Macro expansion inside `$...$` and `$$...$$` before rendering
- [x] `\_` normalised in math contexts; math rendered inside footnote text
- [x] Two-pass label collection — `\label`/`\ref`/`\eqref`/`\cite` resolved before render
- [x] `\begin{algorithm}`/`algorithmic` environments → plain-text pseudocode
- [x] `\begin{lstlisting}`/`verbatim` → CodeBlock with language tag

### Reader UX
- [x] Section-jump navigation: `[` prev section, `]` next section
- [x] Toggleable TOC side panel: `t` — current section tracked and highlighted
- [x] Width-aware reflow when TOC toggles (content_width_for helper, rebuild on toggle/resize)
- [x] Back-navigation stack: `Ctrl+O` to return to previous position after jump
- [x] Paper metadata header bar — title, authors pinned above content
- [x] `?` help overlay showing all block-reader keybindings
- [x] Bookmarks — `m` to toggle, `'` to jump forward; persisted per arXiv ID; amber highlight
- [x] `PageUp`/`PageDown` full-page scroll; `{`/`}` paragraph jump; `H`/`M`/`L` screen top/mid/bottom; `z` center cursor
- [x] `*` search word under cursor; `h`/`l` column cursor movement
- [x] Previous bookmark — `` ` `` cycles bookmarks backward (pairs with `'` forward)
- [x] `y` yank current line to clipboard via OSC 52
- [x] Count prefix for motions — `5j`, `10G`, `3]`, etc.; shown in status bar during entry
- [x] Visual mode — `v` char select, `V` line select; `j`/`k`/`h`/`l` extend; `y` yank selection; `Esc` cancel

### Persistence and integration
- [x] Reading progress persistence per arXiv ID (~/.config/trench/reader_progress.json)
- [x] Wire block-reader into trench — arXiv `Enter` routes to block-reader; TUI suspend/resume
- [x] Abstract quick-view from feed: `Space` shows summary popup without entering reader

### Text styling (complete)
- [x] Bold text — `\textbf{}` → Modifier::BOLD
- [x] Italic — `\textit{}`, `\emph{}` → Modifier::ITALIC
- [x] Underline — `\underline{}` → Modifier::UNDERLINED
- [x] Strikethrough — `\sout{}` → Modifier::CROSSED_OUT
- [x] Monospace — `\texttt{}`, `\verb`, `verbatim` blocks → distinct colour
- [x] Coloured text — `\textcolor{}` → ratatui fg()

### Structure and numbering
- [x] Numbered sections — "1  Introduction", "2.1  Background"
- [x] Numbered theorems — "Theorem 1", "Lemma 2"
- [x] Proof end marker — ∎ at end of proof environments
- [x] Numbered equations — `(1)`, `(2)` right-justified in display math
- [x] Cross-reference resolution — `\ref{eq:elbo}` → equation number or label string
- [x] Citation formatting — `\cite{vaswani}` → `[1]`
- [x] Bibliography / references section rendered (parse_bibliography, clean_bib_entry)

### Environments
- [x] Algorithm / pseudocode blocks — plain text render (parse_algorithmic_body)
- [x] Code listings (`lstlisting`, `verbatim`) — CodeBlock with language tag
- [x] Nested lists — list within list, indent tracks depth (depth * 2 indent in wrap_list_item)
- [x] Table horizontal rules — `\hline`, `\toprule`, `\midrule`, `\bottomrule` as `─────` separators (parse_tabular returns Vec<Block>)

### Math rendering
- [x] Silent garbling fix — backslash in tui-math output triggers strip_latex fallback
- [x] Symbol table (~100 entries) — Greek, calculus, relations, set theory, arrows, operators
- [x] Multi-line align/gather splitting — render_multiline() splits on \\ before tui-math
- [x] Enhanced strip_latex — \frac→a/b, \sqrt→√x, Unicode super/subscripts, recursive unwrap

### Navigation and cross-document UX
- [ ] Cross-reference jumping — Enter on `[ref]` / `Figure 3` jumps to target
- [x] Terminal hyperlinks — `\url{}` and `\href{}` as OSC 8 clickable links (InlineSpan.url field; underline fallback for non-OSC 8 terminals)
- [ ] `[ref]` markers → expandable citations overlay (press Enter to expand inline)

### Hard but achievable
- [ ] Two-column layout — IEEE-style papers; simulate with side-by-side scroll panes
- [ ] Full BibTeX parsing — resolve `\cite{key}` to author/year from bundled .bib file (arXiv includes both)

### Richer document model
- [x] InlineSpan { bold, italic, underline, strikethrough, monospace, color } — in doc-model
- [x] Block::StyledLine(Vec<InlineSpan>) — parse.rs emits this; build_visual_lines handles it
- [x] Block::ListItem { depth, marker, content } — typed list items
- [x] Block::CodeBlock { lang, lines } — verbatim/listing with language tag
- [x] Block::Rule — horizontal separator
- [x] Block::Quote — blockquote / epigraph styling (quote/quotation/epigraph → italic, 4-space indent)
- [x] Bold/italic/monospace/underline/strikethrough/color rendered in block-reader via VisualLineKind::StyledProse

### "Cannot achieve" — to be solved
- [ ] Images and figures — currently: no pixel graphics in TTY/tmux. Path forward: Kitty graphics protocol with tmux passthrough, or sixel, or inline SVG-to-ASCII fallback for simple diagrams
- [ ] Knuth-quality math typesetting — vertical fractions, proper integral limits, stacked scripts. Path forward: render math to small pixel bitmaps via MathJax/KaTeX headless → sixel/kitty inline
- [ ] Small caps (`\textsc{}`) — no terminal equivalent today. Path forward: Unicode small-cap letter substitution (ᴀʙᴄᴅ…) as approximation
- [ ] Multi-column body text — physically possible in terminal; deferred until single-column is perfect
- [ ] Margin notes (`\marginpar{}`) — path forward: collect and display in TOC panel or as footnotes
- [ ] Multi-file LaTeX with custom .sty / class files — path forward: bundle a subset of common packages (amsmath, natbib, algorithm2e) as known macro tables

## README / open source
- [x] Write README for public release
- [x] Add hero screenshot/demo
- [x] Document installation and configuration
- [x] Document keybindings
- [ ] Choose and finalise license (currently AGPL-3.0)

## Philosophy — Beautiful Code & Software

This project strives for beauty in both code and UI. Beauty is not measured in lines 
of code — it comes from simplicity, clarity, and restraint.

### What makes code beautiful

**Minimality** — the best solution uses no more than it needs. If you can remove 
something without breaking meaning, remove it. No wasted parts, no wasted motion.
"I couldn't add anything, I couldn't take anything away." — Brian Kernighan

**Simplicity over cleverness** — prefer the straightforward solution. Clever code 
that nobody can read in six months is not beautiful. Code that reads like prose is.

**Single responsibility** — every function, struct, and module does one thing well. 
If you need to use "and" to describe what something does, split it.

**Consistency** — the codebase should feel like one person wrote it. Naming, 
structure, patterns, and style should be uniform throughout. You should not be able 
to tell which part was written first.

**Brevity** — short functions (ideally visible on one screen). Short modules. 
Short names that are still descriptive. If a function only calls one other function, 
it is getting in the way.

**No repetition (DRY)** — if the same logic appears twice, it belongs in one place. 
Duplication is the enemy of elegance.

**Revelation** — elegant code shows you something about the problem. It makes the 
solution feel inevitable, not accidental.

**Self-documenting** — if code needs a comment to explain what it does, it should 
be rewritten. Comments explain why, not what.

**Security by default** — never trust input. Validate at boundaries. Fail loudly 
and early. Security is not an afterthought — it is part of the design.

### What makes UI beautiful

**Restraint** — show only what is needed. Every element on screen must earn its 
place. Removing is usually better than adding.

**Consistency** — same patterns, same colors, same spacing throughout. The user 
should never be surprised by the interface.

**Shared visual language** — all panes follow the same framing rules:
- No individual box borders on content widgets
- One shared outer border with section titles in the divider: `─── Title ───`
- Border color: `Color::DarkGray`
- Floating overlays/popups keep their own borders
- This rule applies to every pane, now and in the future

**Spatial clarity** — the layout communicates structure. The user always knows 
where they are and how to get somewhere else.

**Typography matters** — monospace, consistent sizing, deliberate use of bold and 
color. Color carries meaning — use it sparingly so it still means something.

**Feynman principle** — if you cannot explain the design decision simply, the 
design is probably wrong.

### The test

Before finishing any piece of work, ask:
- Can I remove anything without losing meaning?
- Does this feel inevitable, or accidental?
- Would a new contributor understand this without being told?
- Does every element earn its place?

If the answer to any of these is no, keep refining.

## Tentative Visual Design Language

Tentative should use a quiet research-interface design language:
- Shared frames and split containers over independently boxed widgets.
- Muted slate borders and separators.
- Section titles embedded into divider/header lines.
- Baby blue for primary accent/actionable content.
- Darker luminous blue for section and column headers.
- Selection should use row/background treatment, not bright borders.
- Footers should be calm command text.
- Repo viewer, chat, and notes should feel structurally consistent with feed/details.
- Reader mode is a separate long-form reading design pass.

## Design Principles

### Pane styling — always follow this rule
All panes must use the shared outer border style:
- No individual `Block::bordered()` on pane content widgets
- One shared outer border enclosing related panes with `Color::DarkGray`
- Section titles in the divider line format: `─── Title ───`
- Floating overlays and popups may keep their own borders
- Consistent across all panes: feed, details, notes, reader, chat, and any future panes
