use std::path::{Path, PathBuf};

/// Resolve the library path in priority order:
///   1. Explicit path (from CLI flag or other source)
///   2. Current directory (if it contains media files or media subdirs)
///   3. Path saved in MediaVault config
pub fn resolve_library(explicit: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(p) = explicit {
        return if p.is_dir() {
            Ok(p)
        } else {
            Err(format!("{} is not a directory", p.display()))
        };
    }

    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    if looks_like_media_dir(&cwd) {
        return Ok(cwd);
    }

    let config = crate::tmdb::load_config();
    if !config.library_path.is_empty() {
        let p = PathBuf::from(&config.library_path);
        if p.is_dir() {
            return Ok(p);
        }
    }

    Err("could not find a media library".into())
}

/// Heuristic: a directory is a media library if it contains video files or
/// subdirectories that contain video files.
pub fn looks_like_media_dir(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else { return false; };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && crate::is_video(&path) {
            return true;
        }
        if path.is_dir() {
            if let Ok(sub) = std::fs::read_dir(&path) {
                if sub.flatten().any(|e| {
                    let p = e.path();
                    p.is_file() && crate::is_video(&p)
                }) {
                    return true;
                }
            }
        }
    }
    false
}
