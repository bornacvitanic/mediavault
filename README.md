# MediaVault

A stateless media management app for Windows. Point it at a folder and it
manages movies and TV shows by writing small human-readable sidecar files
alongside your media — no database, no central store.

## Build

```
cargo build --release
```

The binary is at `target/release/mediavault.exe`.

## Library detection rules

| Folder structure | Detected as |
|---|---|
| `Root/movie.mkv` | Movie (title = filename) |
| `Root/The Matrix/the.matrix.mkv` | Movie (title = folder name) |
| `Root/Breaking Bad/S01E01.mkv` | Show |
| `Root/Breaking Bad/Season 1/S01E01.mkv` | Show with season grouping |

## Sidecar files

MediaVault only ever creates files with these names. It never touches your
video files.

### `movie.watched.toml`
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
# watched_episodes lists relative paths of every fully-watched episode.
# next_up is the episode that will open when you press Continue.
watched_episodes = [
    "Season 1/S01E01.mkv",
    "Season 1/S01E02.mkv",
]
next_up = "Season 1/S01E03.mkv"
```

### `media.comments.md`
Free-form markdown. Edited directly in the app or in any text editor.

### `media.poster.jpg`
Cached poster downloaded from TMDB. Delete this file to force a re-fetch.

## TMDB API key

Get a free key at https://www.themoviedb.org/settings/api

Open **Settings** in the app and paste your key. It is saved to:
```
%APPDATA%\mediavault\config.toml
```

## Architecture

```
mediavault/
├── media_core/        # Pure library — scanning, sidecar I/O, TMDB, all logic
│   └── src/
│       ├── lib.rs
│       ├── models.rs  # Domain types
│       ├── scanner.rs # Directory scanning
│       ├── sidecar.rs # File read/write
│       └── tmdb.rs    # Poster fetching
└── media_gui/         # egui frontend — calls media_core only, no I/O of its own
    └── src/
        └── main.rs
```

`media_core` has no GUI dependency and can be reused for a CLI or TUI frontend.
