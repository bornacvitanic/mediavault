use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::{ExternalSubtitle, MediaEntry, SubtitleTrack};

const SUBTITLE_EXTENSIONS: &[&str] = &["srt", "ass", "ssa", "sub", "idx", "vtt"];

// ── Sidecar cache ────────────────────────────────────────────────────────────

/// Cached subtitle data written to `{video_stem}.media.subtitles.toml`.
#[derive(Debug, Serialize, Deserialize)]
struct SubtitleCache {
    /// mtime of the video file when the cache was written.
    /// If the video file's mtime has changed, the cache is stale.
    video_mtime: DateTime<Utc>,
    #[serde(default)]
    embedded: Vec<SubtitleTrack>,
    #[serde(default)]
    external: Vec<ExternalSubtitle>,
}

fn subtitle_cache_path(video_path: &Path) -> std::path::PathBuf {
    let stem = video_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let dir = video_path.parent().unwrap_or(Path::new("."));
    dir.join(format!("{stem}.media.subtitles.toml"))
}

fn video_mtime(video_path: &Path) -> Option<DateTime<Utc>> {
    std::fs::metadata(video_path)
        .ok()?
        .modified()
        .ok()
        .map(|t| t.into())
}

/// Try to load cached subtitle data. Returns `None` if the cache is missing,
/// unreadable, or stale (video mtime changed).
fn load_subtitle_cache(video_path: &Path) -> Option<SubtitleCache> {
    let cache_path = subtitle_cache_path(video_path);
    let raw = std::fs::read_to_string(&cache_path).ok()?;
    let cache: SubtitleCache = toml::from_str(&raw).ok()?;

    // Invalidate if video file mtime has changed.
    let current_mtime = video_mtime(video_path)?;
    if cache.video_mtime != current_mtime {
        return None;
    }
    Some(cache)
}

/// Write subtitle data to the sidecar cache file.
fn save_subtitle_cache(
    video_path: &Path,
    embedded: &[SubtitleTrack],
    external: &[ExternalSubtitle],
) {
    let Some(mtime) = video_mtime(video_path) else {
        return;
    };
    let cache = SubtitleCache {
        video_mtime: mtime,
        embedded: embedded.to_vec(),
        external: external.to_vec(),
    };
    let Ok(raw) = toml::to_string_pretty(&cache) else {
        return;
    };
    let content = format!(
        "# MediaVault — cached subtitle data\n\
         # Auto-generated from video scan. Delete to force re-scan.\n\n\
         {raw}"
    );
    let _ = std::fs::write(subtitle_cache_path(video_path), content);
}

// ── Lazy loading (public API) ────────────────────────────────────────────────

/// Load subtitles for a single video file, using the sidecar cache when
/// available and falling back to MKV parsing + external file scan.
///
/// Returns `(embedded, external)`.
pub fn load_video_subtitles(video_path: &Path) -> (Vec<SubtitleTrack>, Vec<ExternalSubtitle>) {
    // Try cache first.
    if let Some(cached) = load_subtitle_cache(video_path) {
        return (cached.embedded, cached.external);
    }

    // Cache miss — extract fresh data.
    let embedded = extract_subtitle_tracks(video_path);
    let external = find_external_subtitles(video_path);

    // Write cache for next time.
    save_subtitle_cache(video_path, &embedded, &external);

    (embedded, external)
}

/// Load subtitles for a `MediaEntry`, populating the subtitle fields on all
/// contained video files (single movie or all episodes in a show).
pub fn load_entry_subtitles(entry: &mut MediaEntry) {
    match entry {
        MediaEntry::Movie(m) => {
            let (embedded, external) = load_video_subtitles(&m.video_path);
            m.subtitles = embedded;
            m.external_subs = external;
        }
        MediaEntry::Show(s) => {
            for season in &mut s.seasons {
                for ep in &mut season.episodes {
                    let (embedded, external) = load_video_subtitles(&ep.video_path);
                    ep.subtitles = embedded;
                    ep.external_subs = external;
                }
            }
        }
    }
}

// ── MKV extraction ───────────────────────────────────────────────────────────

/// Extract subtitle track metadata from an MKV file.
/// Returns an empty Vec for non-MKV files or on any read error.
pub fn extract_subtitle_tracks(video_path: &Path) -> Vec<SubtitleTrack> {
    let ext = video_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if ext != "mkv" && ext != "webm" {
        return Vec::new();
    }

    let file = match std::fs::File::open(video_path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let mkv = match matroska::Matroska::open(file) {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };

    mkv.tracks
        .into_iter()
        .filter(|t| t.tracktype == matroska::Tracktype::Subtitle)
        .map(|t| SubtitleTrack {
            track_number: t.number,
            language: t.language.map(|l| format!("{l}")),
            codec_id: t.codec_id,
            name: t.name,
            default: t.default,
            forced: t.forced,
        })
        .collect()
}

// ── External subtitle scan ───────────────────────────────────────────────────

/// Find external subtitle files next to a video file.
///
/// Looks for files in the same directory whose name starts with the video's
/// stem and has a subtitle extension (.srt, .ass, .ssa, .sub, .idx, .vtt).
///
/// Handles common naming patterns:
///   movie.srt              → no language
///   movie.en.srt           → language "en"
///   movie.English.srt      → language "English"
pub fn find_external_subtitles(video_path: &Path) -> Vec<ExternalSubtitle> {
    let Some(parent) = video_path.parent() else {
        return Vec::new();
    };
    let video_stem = video_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();

    let Ok(entries) = std::fs::read_dir(parent) else {
        return Vec::new();
    };

    let mut subs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if !SUBTITLE_EXTENSIONS.contains(&ext.as_str()) {
            continue;
        }
        let filename = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let sub_stem = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();

        // Must start with the video stem (case-insensitive)
        if !sub_stem.to_lowercase().starts_with(&video_stem.to_lowercase()) {
            continue;
        }

        // Extract language from the part between the video stem and the extension.
        // e.g. "movie.en" → ".en" → "en"
        let remainder = &sub_stem[video_stem.len()..];
        let language = remainder
            .strip_prefix('.')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        subs.push(ExternalSubtitle {
            filename,
            language,
            format: ext,
        });
    }

    subs.sort_by(|a, b| a.filename.cmp(&b.filename));
    subs
}
