use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── Video file extensions recognised during scanning ──────────────────────────

pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "avi", "mov", "wmv", "flv", "webm", "m4v", "ts", "m2ts",
];

// ── Core entry types ──────────────────────────────────────────────────────────

/// A single item in the managed library — either a movie or a show.
#[derive(Debug, Clone)]
pub enum MediaEntry {
    Movie(Movie),
    Show(Show),
}

impl MediaEntry {
    pub fn title(&self) -> &str {
        match self {
            MediaEntry::Movie(m) => &m.title,
            MediaEntry::Show(s) => &s.title,
        }
    }

    /// Directory that contains all sidecar files for this entry.
    pub fn base_dir(&self) -> &PathBuf {
        match self {
            MediaEntry::Movie(m) => &m.base_dir,
            MediaEntry::Show(s) => &s.base_dir,
        }
    }

    /// Most recent modification time of any video file belonging to this entry.
    /// Used for "sort by date downloaded / added".
    pub fn latest_video_mtime(&self) -> Option<DateTime<Utc>> {
        match self {
            MediaEntry::Movie(m) => m.video_mtime,
            MediaEntry::Show(s) => s
                .seasons
                .iter()
                .flat_map(|se| se.episodes.iter())
                .filter_map(|ep| ep.video_mtime)
                .max(),
        }
    }

    pub fn poster_cache_path(&self) -> &PathBuf {
        match self {
            MediaEntry::Movie(m) => &m.poster_path,
            MediaEntry::Show(s) => &s.poster_path,
        }
    }
}

// ── Movie ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Movie {
    /// Display name derived from filename or folder name.
    pub title: String,
    /// Folder (or root dir) that owns this movie.
    pub base_dir: PathBuf,
    /// Absolute path to the video file.
    pub video_path: PathBuf,
    pub video_mtime: Option<DateTime<Utc>>,
    /// Persisted state loaded from `movie.watched.toml`.
    pub state: MovieState,
    /// Path where the cached TMDB poster is stored.
    /// Uses `{video_stem}.media.poster.jpg` so root-level movies (which share
    /// a base_dir with other entries) never overwrite each other's posters.
    pub poster_path: PathBuf,
}

/// Persisted, human-editable state written to `movie.watched.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MovieState {
    /// Whether the movie has been watched at least once.
    pub watched: bool,
    /// All watch events, most recent last.
    #[serde(default)]
    pub watch_history: Vec<WatchEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchEvent {
    /// UTC timestamp of when the movie was marked as watched.
    pub watched_at: DateTime<Utc>,
    /// Optional free-form note for this particular viewing (e.g. "watched with Alice").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

// ── Show ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Show {
    pub title: String,
    pub base_dir: PathBuf,
    pub seasons: Vec<Season>,
    /// Persisted state loaded from `show.bookmarks.toml`.
    pub bookmarks: ShowBookmarks,
    /// Path where the cached TMDB poster is stored: `{folder}.media.poster.jpg`.
    pub poster_path: PathBuf,
}

impl Show {
    /// Flat iterator over all episodes across all seasons, in display order.
    pub fn all_episodes(&self) -> impl Iterator<Item = &Episode> {
        self.seasons.iter().flat_map(|s| s.episodes.iter())
    }

    /// Total episode count.
    pub fn episode_count(&self) -> usize {
        self.seasons.iter().map(|s| s.episodes.len()).sum()
    }

    /// Number of episodes marked as watched.
    pub fn watched_count(&self) -> usize {
        self.all_episodes()
            .filter(|ep| self.bookmarks.is_watched(&ep.relative_path))
            .count()
    }

    pub fn is_fully_watched(&self) -> bool {
        self.watched_count() == self.episode_count()
    }
}

/// A season group — either a real subfolder (e.g. `Season 1`) or the synthetic
/// "root" group used when episodes sit directly in the show folder.
#[derive(Debug, Clone)]
pub struct Season {
    /// Display label, e.g. "Season 1" or the subfolder name.
    pub label: String,
    pub episodes: Vec<Episode>,
}

#[derive(Debug, Clone)]
pub struct Episode {
    pub title: String,
    pub video_path: PathBuf,
    pub video_mtime: Option<DateTime<Utc>>,
    /// Path relative to `Show::base_dir`, used as the stable bookmark key so
    /// that renaming the root library dir doesn't invalidate bookmarks.
    pub relative_path: String,
}

/// Persisted, human-editable state written to `show.bookmarks.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShowBookmarks {
    /// Relative paths of episodes that have been fully watched.
    #[serde(default)]
    pub watched_episodes: Vec<String>,

    /// Relative path of the episode to resume next.
    /// Automatically advances to the episode after the last-watched one, but
    /// can be overridden manually in the file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_up: Option<String>,
}

impl ShowBookmarks {
    pub fn is_watched(&self, relative_path: &str) -> bool {
        self.watched_episodes.iter().any(|p| p == relative_path)
    }

    /// Mark an episode as watched and advance `next_up` to the following
    /// episode if one is provided.
    pub fn mark_watched(&mut self, relative_path: &str, following: Option<&str>) {
        if !self.is_watched(relative_path) {
            self.watched_episodes.push(relative_path.to_string());
        }
        // Only advance next_up if it was pointing at this episode or is unset.
        let should_advance = self
            .next_up
            .as_deref()
            .map(|n| n == relative_path)
            .unwrap_or(true);
        if should_advance {
            self.next_up = following.map(str::to_string);
        }
    }

    pub fn mark_unwatched(&mut self, relative_path: &str) {
        self.watched_episodes.retain(|p| p != relative_path);
    }
}

// ── Comments (shared between movies and shows) ────────────────────────────────

/// Loaded from `media.comments.md`. Stored as raw markdown so the user can
/// freely edit the file in any text editor.
#[derive(Debug, Clone, Default)]
pub struct Comments {
    pub markdown: String,
}
