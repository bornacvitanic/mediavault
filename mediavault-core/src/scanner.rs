use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use chrono::{DateTime, Utc};
use natord::compare as natural_compare;

use crate::{
    models::{Episode, MediaEntry, Movie, Season, Show, VIDEO_EXTENSIONS},
    sidecar::{load_movie_state, load_show_bookmarks},
    tmdb::{extract_metadata, extract_metadata_with_episodes, parse_episode},
};

// ── Public entry point ────────────────────────────────────────────────────────

/// Scan `root` and return all recognised media entries.
///
/// Detection rules:
/// - A video file directly in `root`                           → Movie
/// - A direct subfolder containing exactly **one** video file → Movie  
/// - A direct subfolder containing **multiple** video files,
///   or nested season subfolders each containing videos        → Show
///
/// Entries are *not* sorted here; the GUI/CLI layer decides ordering.
pub fn scan_library(root: &Path) -> Vec<MediaEntry> {
    let mut entries = Vec::new();

    let dir_iter = match fs::read_dir(root) {
        Ok(it) => it,
        Err(e) => {
            eprintln!("Failed to read library root {:?}: {}", root, e);
            return entries;
        }
    };

    for entry in dir_iter.flatten() {
        let path = entry.path();

        if path.is_file() && is_video(&path) {
            // A bare video file at the root level is a movie whose base_dir is
            // the root itself. We use the filename (sans extension) as title.
            if let Some(movie) = movie_from_single_file(root, &path) {
                entries.push(MediaEntry::Movie(movie));
            }
        } else if path.is_dir() {
            match classify_subfolder(&path) {
                SubfolderKind::Movie(video_path) => {
                    if let Some(movie) = movie_from_single_file(&path, &video_path) {
                        entries.push(MediaEntry::Movie(movie));
                    }
                }
                SubfolderKind::Show => {
                    if let Some(show) = show_from_dir(&path) {
                        entries.push(MediaEntry::Show(show));
                    }
                }
                SubfolderKind::Empty => {}
            }
        }
    }

    entries
}

// ── Classification ────────────────────────────────────────────────────────────

enum SubfolderKind {
    Movie(PathBuf),
    Show,
    Empty,
}

/// Determine whether a direct child directory represents a movie or a show.
fn classify_subfolder(dir: &Path) -> SubfolderKind {
    // Collect all video files at any depth inside `dir`.
    let videos = collect_videos_recursive(dir);

    match videos.len() {
        0 => SubfolderKind::Empty,
        1 => SubfolderKind::Movie(videos.into_iter().next().unwrap()),
        _ => SubfolderKind::Show,
    }
}

/// Recursively collect all video file paths under `dir`.
fn collect_videos_recursive(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if let Ok(iter) = fs::read_dir(dir) {
        for entry in iter.flatten() {
            let p = entry.path();
            if p.is_file() && is_video(&p) {
                found.push(p);
            } else if p.is_dir() {
                found.extend(collect_videos_recursive(&p));
            }
        }
    }
    found
}

// ── Movie construction ────────────────────────────────────────────────────────

fn movie_from_single_file(base_dir: &Path, video_path: &Path) -> Option<Movie> {
    let raw = stem_title(video_path);
    let metadata = extract_metadata(&raw);
    let title = raw.clone();
    let state = load_movie_state(video_path).unwrap_or_default();
    let poster_path = base_dir.join(format!(
        "{}.media.poster.jpg",
        video_path.file_stem().unwrap_or_default().to_string_lossy()
    ));
    Some(Movie {
        title,
        base_dir: base_dir.to_path_buf(),
        video_mtime: mtime(video_path),
        video_path: video_path.to_path_buf(),
        state,
        poster_path,
        metadata,
    })
}

// ── Show construction ─────────────────────────────────────────────────────────

fn show_from_dir(dir: &Path) -> Option<Show> {
    let title = dir_title(dir);
    let seasons = build_seasons(dir);
    if seasons.is_empty() {
        return None;
    }
    // Collect bare filename stems from all episodes so extract_metadata_with_episodes
    // can find tags common to all files when the folder name alone has none.
    let episode_stems: Vec<String> = seasons
        .iter()
        .flat_map(|s| s.episodes.iter())
        .filter_map(|ep| {
            ep.video_path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .collect();
    let metadata = extract_metadata_with_episodes(&title, &episode_stems);
    let bookmarks = load_show_bookmarks(dir).unwrap_or_default();
    let folder_stem = dir.file_name().unwrap_or_default().to_string_lossy();
    let poster_path = dir.join(format!("{folder_stem}.media.poster.jpg"));
    Some(Show {
        title,
        base_dir: dir.to_path_buf(),
        seasons,
        bookmarks,
        poster_path,
        metadata,
    })
}

/// Build the season/episode tree for a show directory.
///
/// Strategy:
/// 1. Any video files directly in `dir` → synthetic "Episodes" season.
/// 2. Any subdirectories of `dir` that contain videos → one season each,
///    labelled by subfolder name and sorted naturally.
fn build_seasons(dir: &Path) -> Vec<Season> {
    let mut seasons: Vec<Season> = Vec::new();

    let Ok(entries) = fs::read_dir(dir) else {
        return seasons;
    };

    let mut root_videos: Vec<PathBuf> = Vec::new();
    let mut subdirs: Vec<PathBuf> = Vec::new();

    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_file() && is_video(&p) {
            root_videos.push(p);
        } else if p.is_dir() {
            subdirs.push(p);
        }
    }

    // Root-level videos form an unnamed / flat season.
    if !root_videos.is_empty() {
        root_videos.sort_by(|a, b| {
            natural_compare(
                a.file_name().unwrap_or_default().to_str().unwrap_or(""),
                b.file_name().unwrap_or_default().to_str().unwrap_or(""),
            )
        });
        seasons.push(Season {
            label: "Episodes".to_string(),
            episodes: root_videos.iter().map(|p| make_episode(dir, p)).collect(),
        });
    }

    // Each subdirectory that contains videos becomes a season.
    subdirs.sort_by(|a, b| {
        natural_compare(
            a.file_name().unwrap_or_default().to_str().unwrap_or(""),
            b.file_name().unwrap_or_default().to_str().unwrap_or(""),
        )
    });

    for subdir in subdirs {
        let mut vids = collect_videos_direct(&subdir);
        if vids.is_empty() {
            continue;
        }
        vids.sort_by(|a, b| {
            natural_compare(
                a.file_name().unwrap_or_default().to_str().unwrap_or(""),
                b.file_name().unwrap_or_default().to_str().unwrap_or(""),
            )
        });
        let label = dir_title(&subdir);
        seasons.push(Season {
            label,
            episodes: vids.iter().map(|p| make_episode(dir, p)).collect(),
        });
    }

    seasons
}

/// Collect video files directly inside `dir` (non-recursive).
fn collect_videos_direct(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if let Ok(iter) = fs::read_dir(dir) {
        for entry in iter.flatten() {
            let p = entry.path();
            if p.is_file() && is_video(&p) {
                found.push(p);
            }
        }
    }
    found
}

fn make_episode(show_base: &Path, video_path: &Path) -> Episode {
    let raw_stem = stem_title(video_path);
    let parsed = parse_episode(&raw_stem);
    let relative_path = video_path
        .strip_prefix(show_base)
        .unwrap_or(video_path)
        .to_string_lossy()
        .replace('\\', "/");
    Episode {
        title: raw_stem,
        season_num: parsed.season_num,
        episode_num: parsed.episode_num,
        episode_title: parsed.episode_title,
        video_mtime: mtime(video_path),
        video_path: video_path.to_path_buf(),
        relative_path,
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

pub fn is_video(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| VIDEO_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Best-effort title from a file path: filename without extension.
fn stem_title(path: &Path) -> String {
    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

/// Best-effort title from a directory: the directory name.
fn dir_title(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

fn mtime(path: &Path) -> Option<DateTime<Utc>> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()
        .map(|t: SystemTime| t.into())
}
