[![Test](https://github.com/bornacvitanic/mediavault/actions/workflows/rust.yml/badge.svg)](https://github.com/bornacvitanic/mediavault/actions/workflows/rust.yml)
[![dependency status](https://deps.rs/repo/github/bornacvitanic/mediavault/status.svg)](https://deps.rs/repo/github/bornacvitanic/mediavault)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Download](https://img.shields.io/badge/download-releases-blue.svg)](https://github.com/bornacvitanic/mediavault/releases)

# MediaVault

A lightweight watch tracker for your local movie and TV show files. Point it at a folder and it tracks what you've watched, where you left off, and your notes — all stored as small human-readable sidecar files next to your media. No server, no database, no media indexing. Not a media center like Plex or Jellyfin — just watch state tracking that stays out of your way.

## Features

- **Stateless sidecar persistence** — watch state, bookmarks, and comments are stored as human-editable TOML/Markdown files next to your media
- **Three frontends** — GUI (egui), CLI (clap), and TUI (ratatui), all sharing the same core library
- **TMDB poster integration** — automatic poster fetching and caching with a free API key
- **Smart media detection** — movies and shows are auto-detected from folder structure and filenames
- **Torrent filename parsing** — handles scene naming, anime fansubs, unicode lookalikes, and bracket-heavy release tags
- **Fuzzy title matching** — 4-tier scoring (exact, starts-with, word-starts, contains) for CLI queries
- **Watch tracking** — per-movie watched state with timestamped history, per-episode bookmarks with auto-advancing next-up
- **Comments** — free-form Markdown notes per entry, editable in-app or in any text editor

## Roadmap

- [ ] Cross-platform support (Linux, macOS)
- [ ] Bulk operations in CLI
- [ ] Search and filtering in GUI/TUI

## Getting Started

### Requirements

- Rust toolchain (1.70+)
- Windows (Linux/macOS support planned)
- Optional: [TMDB API key](https://www.themoviedb.org/settings/api) for poster fetching

### Installation

1. Clone the repository:
   ```shell
   git clone https://github.com/bornacvitanic/mediavault.git
   cd mediavault
   ```

2. Build all frontends:
   ```shell
   cargo build --release
   ```

3. Binaries are at:
   - `target/release/mediavault.exe` (GUI)
   - `target/release/mediavault-cli.exe` (CLI)
   - `target/release/mediavault-tui.exe` (TUI)

## Usage

### GUI
```shell
mediavault
```
Open **Settings** to set your library path and TMDB API key.

### CLI
```shell
# List all entries
mediavault-cli ls

# Show details for a specific entry
mediavault-cli get "breaking bad"

# Mark a movie as watched
mediavault-cli done "tron legacy"

# Get next episode for a show
mediavault-cli next "dr stone"

# Open entry status
mediavault-cli status "frieren"
```

### TUI
```shell
mediavault-tui
```

## Library Detection Rules

| Folder structure | Detected as |
|---|---|
| `Root/movie.mkv` | Movie (title = filename) |
| `Root/The Matrix/the.matrix.mkv` | Movie (title = folder name) |
| `Root/Breaking Bad/S01E01.mkv` | Show |
| `Root/Breaking Bad/Season 1/S01E01.mkv` | Show with season grouping |

## Sidecar Files

MediaVault only ever creates files with these names. It never touches your video files.

### `{video_stem}.watched.toml`
```toml
# MediaVault — movie watch state
watched = true

[[watch_history]]
watched_at = "2024-11-01T20:30:00Z"
note = "Watched with Alice"
```

### `show.bookmarks.toml`
```toml
# MediaVault — show bookmark state
watched_episodes = [
    "Season 1/S01E01.mkv",
    "Season 1/S01E02.mkv",
]
next_up = "Season 1/S01E03.mkv"
```

### `{video_stem}.media.comments.md`
Free-form markdown. Edited directly in the app or in any text editor.

### `{video_stem}.media.poster.jpg`
Cached poster downloaded from TMDB. Delete this file to force a re-fetch.

## TMDB API Key

Get a free key at https://www.themoviedb.org/settings/api

The key is saved to:
```
%APPDATA%\mediavault\config.toml
```

## Project Structure

```
mediavault/
├── media_core/        # Shared library — scanning, sidecar I/O, TMDB, metadata parsing
│   └── src/
│       ├── lib.rs
│       ├── models.rs  # Domain types
│       ├── scanner.rs # Directory scanning
│       ├── sidecar.rs # Sidecar file read/write
│       ├── tmdb.rs    # TMDB integration and filename parsing
│       ├── player.rs  # System media player launch
│       └── library.rs # Library path resolution
├── media_gui/         # egui desktop frontend
├── media_cli/         # clap CLI frontend
└── media_tui/         # ratatui TUI frontend
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License — see the [LICENSE.md](LICENSE.md) file for details.

## Acknowledgements

- [egui](https://docs.rs/eframe) — immediate mode GUI framework
- [clap](https://docs.rs/clap) — command-line argument parser
- [ratatui](https://docs.rs/ratatui) — terminal user interface framework
- [TMDB](https://www.themoviedb.org/) — movie and TV show metadata
- [serde](https://docs.rs/serde) / [toml](https://docs.rs/toml) — serialization
- [chrono](https://docs.rs/chrono) — date/time handling

## Contact

- Email: borna.cvitanic@gmail.com
- Issues: https://github.com/bornacvitanic/mediavault/issues