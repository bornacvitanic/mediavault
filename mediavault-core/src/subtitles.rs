use std::path::Path;

use crate::models::{ExternalSubtitle, SubtitleTrack};

const SUBTITLE_EXTENSIONS: &[&str] = &["srt", "ass", "ssa", "sub", "idx", "vtt"];

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
