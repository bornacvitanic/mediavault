mod commands;
mod fuzzy;
mod output;
mod player;

use std::path::PathBuf;
use clap::{Parser, Subcommand};

// ── CLI definition ────────────────────────────────────────────────────────────

/// MediaVault — terminal media tracker
///
/// Run with no arguments inside (or pointing at) your media folder for a
/// quick status overview of what you're currently watching.
///
/// FUZZY MATCHING
///   Most commands accept a partial, case-insensitive title instead of the
///   full name. "frie" matches "Frieren: Beyond Journey's End", "matrix"
///   matches "The Matrix". If multiple entries match you'll be shown the
///   candidates and asked to be more specific.
///
/// EXAMPLES
///   mediavault                       Show in-progress entries and next episodes
///   mediavault next                  Resume the most recently touched entry
///   mediavault next frie             Play next episode of Frieren
///   mediavault next matrix           Play The Matrix
///   mediavault done frie             Mark next unwatched Frieren episode as watched
///   mediavault done frie s01e04      Mark a specific episode as watched
///   mediavault undo frie             Unmark the last watched Frieren episode
///   mediavault ls                    List everything in the library
///   mediavault ls --watching         Only in-progress entries
///   mediavault ls frie               Episode list and progress for Frieren
///   mediavault note frie             Open Frieren notes in $EDITOR
///   mediavault note frie --show      Print existing notes to stdout
#[derive(Parser)]
#[command(
    name = "mediavault",
    bin_name = "mv",
    version,
    about = "Terminal media tracker",
    long_about = None,
    after_help = "\
TIPS
  • Run `mediavault` with no args inside your media folder — it auto-detects
    the library from the current directory or your saved config.
  • `mediavault next` with no title resumes the most recently touched entry.
  • Pipe `mediavault next frie` to a player: mpv \"$(mediavault next frie)\"
  • All sidecar files (.watched.toml, .media.comments.md) are human-editable
    plain text sitting next to your media files.
"
)]
struct Cli {
    /// Path to media library. Defaults to current directory if it contains
    /// media, otherwise falls back to the path saved in config.
    #[arg(long, short, global = true, value_name = "PATH")]
    library: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Play the next unwatched episode or movie, auto-marking it watched.
    ///
    /// With no TITLE, resumes the most recently touched in-progress entry.
    /// With a TITLE, fuzzy-matches against your library.
    ///
    /// When stdout is not a terminal (pipe mode), prints the video path
    /// instead of launching a player — useful for: mpv "$(mv next frie)"
    ///
    /// Examples:
    ///   mv next                  Resume most recent
    ///   mv next frie             Play next Frieren episode
    ///   mv next matrix           Play The Matrix
    ///   mv next frie s01e06      Play a specific episode
    #[command(visible_alias = "n")]
    Next {
        /// Partial title to match (case-insensitive)
        title: Option<String>,
        /// Specific episode, e.g. s01e04 or s1e4
        episode: Option<String>,
        /// Print path only, do not launch player
        #[arg(long)]
        path_only: bool,
    },

    /// Mark an episode or movie as watched.
    ///
    /// With no episode, marks the next unwatched episode of a show,
    /// or the movie itself. Use `undo` to reverse.
    ///
    /// Examples:
    ///   mv done frie             Mark next Frieren episode watched
    ///   mv done frie s01e04      Mark a specific episode watched
    ///   mv done matrix           Mark The Matrix watched
    ///   mv done frie --all       Mark all Frieren episodes watched
    #[command(visible_alias = "d", visible_alias = "watch")]
    Done {
        /// Partial title to match
        title: String,
        /// Specific episode, e.g. s01e04
        episode: Option<String>,
        /// Mark all episodes watched (shows only)
        #[arg(long)]
        all: bool,
    },

    /// Unmark the last watched episode or movie.
    ///
    /// Examples:
    ///   mv undo frie             Unmark last watched Frieren episode
    ///   mv undo matrix           Mark The Matrix unwatched
    #[command(visible_alias = "u")]
    Undo {
        /// Partial title to match
        title: String,
        /// Specific episode to unmark, e.g. s01e04
        episode: Option<String>,
    },

    /// List library entries or show episode detail for one entry.
    ///
    /// With no TITLE, lists all entries with progress. With a TITLE,
    /// shows the full episode list for that show or movie detail.
    ///
    /// Examples:
    ///   mv ls                    List everything
    ///   mv ls --watching         Only in-progress entries
    ///   mv ls --unwatched        Only unwatched entries
    ///   mv ls --watched          Only finished entries
    ///   mv ls frie               Episode list for Frieren
    ///   mv ls --movies           Movies only
    ///   mv ls --shows            Shows only
    #[command(visible_alias = "l", visible_alias = "list")]
    Ls {
        /// Partial title — shows detail for that entry
        title: Option<String>,
        /// Only in-progress entries
        #[arg(long, conflicts_with_all = ["unwatched", "watched"])]
        watching: bool,
        /// Only unwatched entries
        #[arg(long, conflicts_with_all = ["watching", "watched"])]
        unwatched: bool,
        /// Only fully watched entries
        #[arg(long, conflicts_with_all = ["watching", "unwatched"])]
        watched: bool,
        /// Movies only
        #[arg(long, conflicts_with = "shows")]
        movies: bool,
        /// Shows only
        #[arg(long, conflicts_with = "movies")]
        shows: bool,
    },

    /// Open or print notes for an entry.
    ///
    /// Without --show, opens notes in $EDITOR (falling back to nano/notepad).
    /// With --show, prints existing notes to stdout.
    ///
    /// Examples:
    ///   mv note frie             Edit Frieren notes in $EDITOR
    ///   mv note frie --show      Print existing notes
    ///   mv note matrix           Edit movie notes
    #[command(visible_alias = "notes")]
    Note {
        /// Partial title to match
        title: String,
        /// Print notes to stdout instead of opening editor
        #[arg(long, short)]
        show: bool,
    },

    /// Show library summary and in-progress entries (default when no command given).
    ///
    /// This is also what runs when you invoke mediavault with no arguments.
    ///
    /// Examples:
    ///   mv status
    ///   mv                       (same thing)
    #[command(visible_alias = "s")]
    Status,
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();

    let library = match resolve_library(cli.library) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!("hint:  run from your media folder, or pass --library <path>");
            std::process::exit(1);
        }
    };

    let entries = media_core::scan_library(&library);

    let result = match cli.command.unwrap_or(Command::Status) {
        Command::Status => commands::status::run(&entries),
        Command::Next { title, episode, path_only } =>
            commands::next::run(&entries, title.as_deref(), episode.as_deref(), path_only),
        Command::Done { title, episode, all } =>
            commands::done::run(&entries, &title, episode.as_deref(), all),
        Command::Undo { title, episode } =>
            commands::undo::run(&entries, &title, episode.as_deref()),
        Command::Ls { title, watching, unwatched, watched, movies, shows } =>
            commands::ls::run(&entries, title.as_deref(), watching, unwatched, watched, movies, shows),
        Command::Note { title, show } =>
            commands::note::run(&entries, &title, show),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

// ── Library resolution ────────────────────────────────────────────────────────

/// Resolve the library path in priority order:
///   1. --library flag
///   2. Current directory (if it contains media files or media subdirs)
///   3. Path saved in MediaVault config
fn resolve_library(flag: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(p) = flag {
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

    // Fall back to saved config path
    let config = media_core::tmdb::load_config();  // shared config.toml
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
fn looks_like_media_dir(dir: &std::path::Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else { return false; };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && media_core::is_video(&path) {
            return true;
        }
        if path.is_dir() {
            if let Ok(sub) = std::fs::read_dir(&path) {
                if sub.flatten().any(|e| {
                    let p = e.path();
                    p.is_file() && media_core::is_video(&p)
                }) {
                    return true;
                }
            }
        }
    }
    false
}
