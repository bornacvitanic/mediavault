use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── Video file extensions recognised during scanning ──────────────────────────

pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "avi", "mov", "wmv", "flv", "webm", "m4v", "ts", "m2ts",
];

// ── Core entry types ──────────────────────────────────────────────────────────

/// A single item in the managed library — either a movie or a show.
#[derive(Debug, Clone, Serialize)]
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

    pub fn metadata(&self) -> &MediaMetadata {
        match self {
            MediaEntry::Movie(m) => &m.metadata,
            MediaEntry::Show(s) => &s.metadata,
        }
    }

    pub fn poster_cache_path(&self) -> &PathBuf {
        match self {
            MediaEntry::Movie(m) => &m.poster_path,
            MediaEntry::Show(s) => &s.poster_path,
        }
    }

    /// Path to the comments sidecar for this entry.
    /// Movies use `{video_stem}.media.comments.md` next to the video file so
    /// root-level movies sharing a base_dir don't overwrite each other.
    /// Shows use `media.comments.md` inside their folder (no collision risk).
    pub fn comments_path(&self) -> std::path::PathBuf {
        match self {
            MediaEntry::Movie(m) => {
                let stem = m.video_path.file_stem().unwrap_or_default().to_string_lossy();
                let dir = m.video_path.parent().unwrap_or(&m.base_dir);
                dir.join(format!("{stem}.media.comments.md"))
            }
            MediaEntry::Show(s) => s.base_dir.join("media.comments.md"),
        }
    }
}

// ── Movie ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Movie {
    /// Display name derived from filename or folder name.
    pub title: String,
    /// Folder (or root dir) that owns this movie.
    pub base_dir: PathBuf,
    /// Absolute path to the video file.
    pub video_path: PathBuf,
    pub video_mtime: Option<DateTime<Utc>>,
    /// Persisted state loaded from `{video_stem}.watched.toml`.
    pub state: MovieState,
    /// Path where the cached TMDB poster is stored.
    /// Uses `{video_stem}.media.poster.jpg` so root-level movies (which share
    /// a base_dir with other entries) never overwrite each other's posters.
    pub poster_path: PathBuf,
    /// Metadata extracted from the raw filename at scan time.
    pub metadata: MediaMetadata,
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

#[derive(Debug, Clone, Serialize)]
pub struct Show {
    pub title: String,
    pub base_dir: PathBuf,
    pub seasons: Vec<Season>,
    /// Persisted state loaded from `show.bookmarks.toml`.
    pub bookmarks: ShowBookmarks,
    /// Path where the cached TMDB poster is stored: `{folder}.media.poster.jpg`.
    pub poster_path: PathBuf,
    /// Metadata extracted from the folder name at scan time.
    pub metadata: MediaMetadata,
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
#[derive(Debug, Clone, Serialize)]
pub struct Season {
    /// Display label, e.g. "Season 1" or the subfolder name.
    pub label: String,
    pub episodes: Vec<Episode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Episode {
    /// Raw filename stem (fallback display if parsing fails).
    pub title: String,
    /// Season number extracted from filename, e.g. 1 for S01.
    pub season_num: u32,
    /// Episode number extracted from filename, e.g. 4 for E04.
    pub episode_num: u32,
    /// Human-readable episode title extracted from filename, if present.
    pub episode_title: Option<String>,
    pub video_path: PathBuf,
    pub video_mtime: Option<DateTime<Utc>>,
    /// Path relative to `Show::base_dir`, used as the stable bookmark key so
    /// that renaming the root library dir doesn't invalidate bookmarks.
    pub relative_path: String,
}

impl Episode {
    /// Best display label: "S01E04 · Title" or "S01E04" or raw title.
    pub fn display_label(&self) -> String {
        if self.episode_num > 0 {
            let code = format!("S{:02}E{:02}", self.season_num, self.episode_num);
            match &self.episode_title {
                Some(t) if !t.is_empty() => format!("{code}  {t}"),
                _ => code,
            }
        } else {
            self.title.clone()
        }
    }
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

// ── Extracted file metadata ───────────────────────────────────────────────────

/// Metadata extracted from a raw filename at scan time.
/// Used to show tags on hover and in the detail panel.
#[derive(Debug, Clone, Default, Serialize)]
pub struct MediaMetadata {
    /// Clean human-readable title (noise stripped).
    pub clean_title: String,
    /// 4-digit year, if found in the filename.
    pub year: Option<u32>,
    /// Resolution string, e.g. "4K", "1080p", "720p".
    pub resolution: Option<String>,
    /// Source, e.g. "BluRay", "WEB-DL", "WEBRip".
    pub source: Option<String>,
    /// HDR format, e.g. "HDR", "HDR10", "DV".
    pub hdr: Option<String>,
    /// Codec, e.g. "x265", "x264", "HEVC".
    pub codec: Option<String>,
    /// Season number and display label, e.g. (1, "S1"). Present when the
    /// folder name contains an explicit season indicator like S01 or "Season 2".
    pub season: Option<(u32, String)>,
}

impl MediaMetadata {
    /// Returns all present tags as short display strings, in a sensible order.
    pub fn tags(&self) -> Vec<String> {
        let mut tags = Vec::new();
        if let Some((_n, label)) = &self.season { tags.push(label.clone()); }
        if let Some(y) = self.year { tags.push(y.to_string()); }
        if let Some(r) = &self.resolution { tags.push(r.clone()); }
        if let Some(s) = &self.source { tags.push(s.clone()); }
        if let Some(h) = &self.hdr { tags.push(h.clone()); }
        if let Some(c) = &self.codec { tags.push(c.clone()); }
        tags
    }
}
