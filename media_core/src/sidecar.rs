/// Sidecar file I/O — reads and writes the human-editable state files that live
/// alongside the media. These files are the sole persistence mechanism; the app
/// itself holds no database.
///
/// File layout per entry:
/// ```
///   <base_dir>/
///     movie.watched.toml     ← MovieState  (movies only)
///     show.bookmarks.toml    ← ShowBookmarks (shows only)
///     media.comments.md      ← raw markdown comments (both)
///     media.poster.jpg       ← cached TMDB poster (both, optional)
/// ```
use std::{fs, path::Path};

use crate::models::{Comments, MovieState, ShowBookmarks};

const MOVIE_STATE_FILE: &str = "movie.watched.toml";
const SHOW_BOOKMARKS_FILE: &str = "show.bookmarks.toml";
const COMMENTS_FILE: &str = "media.comments.md";

// ── Movie state ───────────────────────────────────────────────────────────────

pub fn load_movie_state(base_dir: &Path) -> Option<MovieState> {
    let path = base_dir.join(MOVIE_STATE_FILE);
    let raw = fs::read_to_string(&path).ok()?;
    match toml::from_str(&raw) {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("Failed to parse {:?}: {}", path, e);
            None
        }
    }
}

pub fn save_movie_state(base_dir: &Path, state: &MovieState) -> std::io::Result<()> {
    let path = base_dir.join(MOVIE_STATE_FILE);
    let raw = toml::to_string_pretty(state)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    let content = format!(
        "# MediaVault — movie watch state\n\
         # You can edit this file manually. It will be re-read on next launch.\n\n\
         {raw}"
    );
    fs::write(path, content)
}

// ── Show bookmarks ────────────────────────────────────────────────────────────

pub fn load_show_bookmarks(base_dir: &Path) -> Option<ShowBookmarks> {
    let path = base_dir.join(SHOW_BOOKMARKS_FILE);
    let raw = fs::read_to_string(&path).ok()?;
    match toml::from_str(&raw) {
        Ok(b) => Some(b),
        Err(e) => {
            eprintln!("Failed to parse {:?}: {}", path, e);
            None
        }
    }
}

pub fn save_show_bookmarks(base_dir: &Path, bookmarks: &ShowBookmarks) -> std::io::Result<()> {
    let path = base_dir.join(SHOW_BOOKMARKS_FILE);
    let raw = toml::to_string_pretty(bookmarks)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    let content = format!(
        "# MediaVault — show bookmark state\n\
         # watched_episodes lists relative paths of every fully-watched episode.\n\
         # next_up is the episode that will open when you press Continue.\n\
         # You can edit both fields manually.\n\n\
         {raw}"
    );
    fs::write(path, content)
}

// ── Comments ──────────────────────────────────────────────────────────────────

pub fn load_comments(base_dir: &Path) -> Comments {
    let path = base_dir.join(COMMENTS_FILE);
    let markdown = fs::read_to_string(path).unwrap_or_default();
    Comments { markdown }
}

pub fn save_comments(base_dir: &Path, comments: &Comments) -> std::io::Result<()> {
    let path = base_dir.join(COMMENTS_FILE);
    fs::write(path, &comments.markdown)
}
