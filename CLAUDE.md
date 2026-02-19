# CLAUDE.md — Songindex

## Purpose

Native macOS desktop app for indexing and managing a guitar/ukulele teaching material collection. Scans a folder tree for PDF sheet music, stores metadata in SQLite, auto-tags based on folder paths, and provides search/filter/tag-editing through a native GUI.

## Tech Stack

- **Language:** Rust (edition 2021)
- **GUI:** eframe/egui 0.29 — immediate-mode native GUI, no web server
- **Database:** rusqlite 0.31 (bundled SQLite, WAL mode)
- **File watching:** notify 6 — watches parent directory for PDF changes
- **Serialization:** serde/serde_json (for data structs)
- **File traversal:** walkdir 2

## Build & Run

```bash
cargo run          # debug build, launches GUI window
cargo build --release   # optimized binary at target/release/songindex
```

Must be run from the `songindex/` directory — it uses `std::env::current_dir().parent()` as the base directory to scan for PDFs.

## Architecture

```
src/
├── main.rs      # Entry point: init DB, scan, start watcher, launch eframe
├── db.rs        # Database layer: schema, CRUD, queries, stats
├── scanner.rs   # Folder scanning, filename parsing, auto-tagging, file watcher
└── ui.rs        # egui UI: search, filters, song list, tag/edit modals
```

### main.rs
Minimal entry point. Opens SQLite DB, runs initial scan, starts file watcher on a background thread, launches the eframe native window (900x700).

### db.rs
All database interaction. Key types:
- `Song`, `TagInfo`, `TagGroup`, `TagEntry`, `Stats`, `SortMode`
- `init_db()` — creates tables (songs, tags, song_tags) and indices
- `query_songs()` — parameterized search with text filter, tag filter (OR within category, AND across categories), audio/untagged toggles, sorting
- `get_or_create_tag()`, `add_tag_to_song()`, `remove_tag_from_song()` — tag management
- `update_song()` — edit title/artist
- `get_all_tags()` — grouped by category in display order
- `get_stats()` — counts for header display

### scanner.rs
- `parse_filename()` — extracts title and artist from "Artist - Title.pdf" patterns
- `infer_tags()` — auto-tags based on folder path patterns (e.g. "E-Gitarre" folder -> instrument:E-Gitarre)
- `find_audio_match()` — checks `00 gitarre/0. Songs/2. Audios/` for matching MP3/WAV/M4A
- `scan_directory()` — full scan: inserts new PDFs, removes stale entries, cleans orphaned tags
- `start_watcher()` — spawns a background thread with `notify::RecommendedWatcher`, sends refresh signals to the UI via `std::sync::mpsc`

### ui.rs
Implements `eframe::App` for `SongIndexApp`. Layout:
1. **Header:** title + stats (total songs, with audio, untagged)
2. **Search bar** + Rescan button
3. **Filter rows:** selectable labels per category (instrument, schwierigkeit, stil, technik) + extras (Nur mit Audio, Ohne Tags)
4. **Toolbar:** result count + sort dropdown
5. **Song list:** scrollable cards with title, artist, colored tag chips, file path, action buttons
6. **Modals:** tag add (with category dropdown + quick-add existing tags), song edit (title/artist), tag remove confirmation

Tag chip colors by category:
- instrument: green (#2d6a4f)
- schwierigkeit: orange (#e76f51)
- stil: blue (#457b9d)
- technik: purple (#6d597a)
- artist: brown (#bc6c25)

## Database Schema

```sql
songs (id, titel, artist, dateipfad UNIQUE, dateiname, has_audio, audio_pfad, created_at, updated_at)
tags (id, kategorie, wert, UNIQUE(kategorie, wert))
song_tags (song_id, tag_id, auto_generated, PRIMARY KEY(song_id, tag_id))
```

## Auto-Tag Rules

Defined in `scanner.rs` as `AUTO_TAGS`. Path-based pattern matching:
- Folder "E-Gitarre" -> instrument:E-Gitarre
- Folder "Zupfen" -> technik:Fingerpicking
- Folder "Anfaenger" or "Kinderlieder" -> schwierigkeit:Anfänger
- Folder "Moderne Popsongs" -> stil:Pop
- Default: anything in "00 gitarre/" without an instrument tag -> instrument:Akustik-Gitarre

## LaunchAgent

`com.songindex.plist` — can be symlinked to `~/Library/LaunchAgents/` for auto-start at login. Runs the release binary with the songindex directory as working directory.

## Key Behaviors

- Scans parent directory recursively for PDFs on startup
- Skips hidden files/dirs and the `songindex/` directory itself
- File watcher detects new/removed PDFs and updates DB + UI automatically
- "Datei öffnen" uses macOS `open` command to launch PDFs in default viewer
- Filter logic: OR within a category, AND across categories
- Tag removal prompts for confirmation
- Orphaned tags are cleaned up automatically
