# AGENTS.md

This file provides guidance to Codex (Codex.ai/code) when working with code in this repository.

## Commands

### /checkpoint
Saves current progress with a commit message.
Usage: /checkpoint "description of what was done"
Command: git add -A && git commit -m "$1"

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
