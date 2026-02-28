/// Sidecar file I/O — reads and writes the human-editable state files that live
/// alongside the media. These files are the sole persistence mechanism; the app
/// itself holds no database.
///
/// File layout per entry:
/// ```
///   <dir>/
///     {video_stem}.watched.toml  ← MovieState  (per video file, not per dir)
///     show.bookmarks.toml        ← ShowBookmarks (shows only)
///     media.comments.md          ← raw markdown comments (both)
///     {video_stem}.media.poster.jpg ← cached TMDB poster (both, optional)
/// ```
///
/// Movie state is keyed on the video filename stem rather than the directory
/// so that multiple movies in the same root folder don't collide.
use std::{fs, path::Path};

use crate::models::{Comments, MovieState, ShowBookmarks};

// Movie state filename is derived from the video stem at call time — see load/save_movie_state.
const SHOW_BOOKMARKS_FILE: &str = "show.bookmarks.toml";
const COMMENTS_FILE: &str = "media.comments.md";
const COMMENTS_FILE_SUFFIX: &str = ".media.comments.md";

// ── Movie state ───────────────────────────────────────────────────────────────

fn movie_state_path(video_path: &Path) -> std::path::PathBuf {
    let stem = video_path.file_stem().unwrap_or_default().to_string_lossy();
    let dir = video_path.parent().unwrap_or(Path::new("."));
    dir.join(format!("{stem}.watched.toml"))
}

pub fn load_movie_state(video_path: &Path) -> Option<MovieState> {
    let path = movie_state_path(video_path);
    let raw = fs::read_to_string(&path).ok()?;
    match toml::from_str(&raw) {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("Failed to parse {:?}: {}", path, e);
            None
        }
    }
}

pub fn save_movie_state(video_path: &Path, state: &MovieState) -> std::io::Result<()> {
    let path = movie_state_path(video_path);
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

/// Comments path for a show or single-folder movie (uses base_dir).
pub fn comments_path_dir(base_dir: &Path) -> std::path::PathBuf {
    base_dir.join(COMMENTS_FILE)
}

/// Comments path for a root-level movie (uses video stem to avoid collisions).
pub fn comments_path_video(video_path: &Path) -> std::path::PathBuf {
    let stem = video_path.file_stem().unwrap_or_default().to_string_lossy();
    let dir = video_path.parent().unwrap_or(Path::new("."));
    dir.join(format!("{stem}{COMMENTS_FILE_SUFFIX}"))
}

/// Load comments from an explicit path (used when the path is pre-computed
/// via `MediaEntry::comments_path()` to handle per-stem movie sidecars).
pub fn load_comments_from_path(path: &Path) -> Comments {
    let raw = match fs::read_to_string(path) {
        Ok(r) => r,
        Err(_) => return Comments::default(),
    };
    Comments { markdown: raw }
}

/// Save comments to an explicit path.
pub fn save_comments_to_path(path: &Path, comments: &Comments) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, &comments.markdown)
}

pub fn load_comments(base_dir: &Path) -> Comments {
    let path = base_dir.join(COMMENTS_FILE);
    let markdown = fs::read_to_string(path).unwrap_or_default();
    Comments { markdown }
}

pub fn save_comments(base_dir: &Path, comments: &Comments) -> std::io::Result<()> {
    let path = base_dir.join(COMMENTS_FILE);
    fs::write(path, &comments.markdown)
}
