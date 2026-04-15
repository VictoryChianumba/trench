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
cargo run --release -- test-data/pdf/pdfreference1.7old.pdf

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

# Build tentative separately
cargo build -p tentative --release
```

Rust edition: 2024, MSRV: 1.88. The `ci.sh` script uses the nightly toolchain for `cargo fix`, `cargo udeps`, and `cargo audit`.

## Workspace Structure

```
hygg/              → main binary: arg parsing, doc conversion → cli-text-reader
cli-text-reader/   → the TUI reader (all editor logic lives here)
cli-pdf-to-text/   → PDF → plain text conversion
cli-epub-to-text/  → EPUB → plain text conversion
cli-justify/       → text justification/wrapping
hygg-shared/       → shared utilities
redirect-stderr/   → stderr redirection helper
tentative/         → separate binary: AI research feed aggregator TUI
```

**hygg pipeline**: arg parsing → OCR (optional, via `ocrmypdf`) → format conversion (PDF/EPUB/pandoc) → `cli-justify::justify()` → `cli-text-reader::run_cli_text_reader()`.

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
- TTS uses ElevenLabs API. Config (`ELEVENLABS_API_KEY`, `VOICE_ID`, `PLAYBACK_SPEED`) lives in `~/.config/hygg/.env`.

**Config** (`src/config.rs`): loaded from `~/.config/hygg/.env` via `dotenvy`. Call `load_config()` at startup; `save_config()` persists changes.

**Persistence**: Progress saved per-document using a hash of the document content (`src/progress.rs`). Bookmarks and highlights also keyed by document hash (`src/bookmarks.rs`, `src/highlights.rs`). All files live under `~/.config/hygg/`.

**Buffers**: The editor supports multiple `BufferState` buffers (used for split-view command output). Buffer 0 is always the document. Active buffer accessed via `self.active_buffer` index.

**Display**: `draw_content_buffered` renders to a `Vec<u8>` then flushes in one write to minimize flicker. Status line rendered separately by `draw_status_line` / `draw_status_line_buffered`.

**Key conventions**:
- All editor methods are `impl Editor` — no separate structs for subsystems.
- Handler functions return `Result<Option<bool>, ...>`: `Some(true)` = quit, `Some(false)` = handled (stop propagation), `None` = not handled (continue to next handler).
- `self.offset` = first visible line index; `self.cursor_y` = cursor row on screen; `self.offset + self.cursor_y` = current doc line.

## tentative Architecture

A separate TUI binary (`tentative/src/main.rs`) that aggregates AI research feeds. No async — uses `std::sync::mpsc` and `reqwest::blocking` throughout.

### Data model (`src/models/`)

`FeedItem` is the central type: `id`, `title`, `source_platform`, `content_type`, `domain_tags`, `signal` (Primary/Secondary/Tertiary), `published_at`, `authors`, `summary_short`, `workflow_state` (Inbox/Skimmed/Queued/DeepRead/Archived), `url`, `upvote_count`. `upvote_count` has `#[serde(default)]` for cache backward-compatibility.

`FeedItem::compute_signal()` derives signal from platform and upvote count. `map_arxiv_category()` and `detect_subtopics()` live in `src/models/categories.rs`.

### Ingestion pipeline (`src/ingestion/`)

Background thread in `main.rs` fetches all sources sequentially then runs enrichment:

1. `arxiv::fetch()` — arXiv Atom API (cs.LG + cs.AI + stat.ML). Maps category codes via `map_arxiv_category()`, detects subtopics via `detect_subtopics()`.
2. `huggingface::fetch()` — Scrapes HF daily papers page (two-pass: h3 for titles, entity-encoded JSON for upvotes/authors), then makes one batched arXiv API call to fill `summary_short` for all items.
3. `rss::fetch()` — Generic RSS 2.0 / Atom parser for OpenAI blog, DeepMind blog, Import AI, The Batch. Handles CDATA via `Event::CData`. Anthropic has no RSS feed and is intentionally skipped.
4. `semantic_scholar::enrich()` — Enriches arXiv items with citation counts and fields of study. 7-day TTL cache at `~/.config/tentative/enrichment_cache.json`. Entries with empty `fields_of_study` are invalidated on load.

Each source sends `FetchMessage::Items(Vec<FeedItem>)` + `FetchMessage::SourceComplete(name)` over mpsc. After all sources, sends enriched batch + `AllComplete`.

### App state and merge logic (`src/app.rs`)

`App::process_incoming()` drains the channel each frame (non-blocking `try_recv` loop):
- **URL dedup**: overwrites cached item with fresh data; workflow state comes from `persisted_states` (keyed by URL).
- **ArXiv ID dedup**: collapses HF and arXiv entries for the same paper — arXiv entry wins. The HF entry's `workflow_state` is preserved onto the arXiv entry when replacing.

Items are sorted by `published_at` descending after each batch. Cache is written to `~/.config/tentative/cache.json` immediately.

### Store (`src/store/`)

- `store::load()` / `store::save()` — workflow states, keyed by URL, at `~/.config/tentative/state.json`.
- `store::cache` — full `Vec<FeedItem>` cache, loaded at startup so the TUI is populated before network fetches complete.
- `store::enrichment_cache` — Semantic Scholar results, 7-day TTL via Julian Day Number arithmetic (no chrono).

### UI (`src/ui/layout.rs`)

Single `draw(frame, app)` entry point. Feed view: tab bar → search row → item table + details panel → status bar with braille spinner during loading. Reader view: full-screen content with header bar. Details panel shows upvote count for HuggingFace items.

---

# Tentative — TODO Checklist

## Bugs to fix
- [ ] Voice mode broken in hygg rewrite (fix after ElevenLabs credits topped up)
- [ ] Chat scrolling not smooth — key repeat and trackpad inertia (partially fixed)
- [ ] Raw ANSI escape codes leaking into right pane in reader mode
- [ ] Notes opening on vim `n` keypress in reader mode (fix with leader key)

## Leader key (Ctrl+T) — app-wide
- [ ] Implement Ctrl+T as global leader key for all Tentative keybindings
- [ ] Hygg vim keybindings remain unchanged (no leader needed)
- [ ] Update footer to always show `Ldr: ctrl+t` and `Ldr+[key]` for all bindings
- [ ] Full keybinding descriptions reserved for help screen only

## Chat panel redesign
- [ ] Move chat from right pane to bottom panel (Feynman-style)
- [ ] Chat panel height = one full screen height, fixed (not additive)
- [ ] Chat streams below main panes, scrollable within its one-screen window
- [ ] `z` moves chat panel to top of application (same content, same scroll, just repositioned)
- [ ] `z` again moves it back to bottom
- [ ] Subtle background color difference to separate chat from main panes (Feynman-style)
- [ ] Clean minimal message style — no heavy borders on messages
- [ ] User messages plain, assistant messages slightly dimmed/indented

## Hygg integration
- [ ] Step 10: Wire rewritten cli-text-reader into Tentative left pane (in progress)
- [ ] Reader mode: full width when no right panel active
- [ ] Reader mode: 60/40 split when notes or chat active
- [ ] Voice mode: fix wiring after ElevenLabs credits topped up
- [ ] Floating hygg reader popup — open selected paper in a centered overlay (Ldr+Enter or dedicated key) without leaving the feed view; dismissible with Esc
- [ ] Secondary split view — toggle a persistent hygg pane alongside the feed list (Ldr+v or similar); user can switch focus between feed and reader pane independently; reader pane retains scroll position when focus returns

## Notes
- [ ] Notes accessible from reader mode via Ldr+n
- [ ] Notes panel opens in right pane alongside reader

## Help screen
- [ ] Design and implement full help screen
- [ ] Show all keybindings with full Ctrl+T leader notation
- [ ] Accessible via Ldr+?

## Source discovery (agent-based)
- [ ] Agent-assisted source discovery — ask model to find sources on a topic
- [ ] Trending/popularity filter using existing signals + web search
- [ ] Default sources remain, agents are supplementary

## UI polish
- [ ] Themes system — allow user to change color scheme
- [ ] Settings screen: add theme selection
- [ ] Overall UI improvements (ongoing)

## Semantic Scholar
- [ ] Fix rate limiting (currently hitting cap immediately)
- [ ] Apply for proper API key
- [ ] Re-enable enrichment once key obtained

## Hygg rewrite (parallel agents)
- [ ] CC plan executing on branch hygg-rewrite-codex (stages 6-9 complete)
- [ ] Codex plan executing on same branch (stages 1-5 complete)
- [ ] Compare both approaches when complete
- [ ] Wire winning approach into Tentative (step 10)

## README / open source
- [ ] Write README for public release
- [ ] Add hero screenshot/demo
- [ ] Document installation and configuration
- [ ] Document keybindings
- [ ] Choose license (currently AGPL-3.0)

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

## Design Principles

### Pane styling — always follow this rule
All panes must use the shared outer border style:
- No individual `Block::bordered()` on pane content widgets
- One shared outer border enclosing related panes with `Color::DarkGray`
- Section titles in the divider line format: `─── Title ───`
- Floating overlays and popups may keep their own borders
- Consistent across all panes: feed, details, notes, reader, chat, and any future panes
