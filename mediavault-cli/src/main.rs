mod commands;
mod fuzzy;
mod output;
mod player;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

// ── CLI definition ────────────────────────────────────────────────────────────

/// MediaVault CLI — terminal media tracker
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
///   mediavault-cli                       Show in-progress entries and next episodes
///   mediavault-cli next                  Resume the most recently touched entry
///   mediavault-cli next frie             Play next episode of Frieren
///   mediavault-cli next matrix           Play The Matrix
///   mediavault-cli done frie             Mark next unwatched Frieren episode as watched
///   mediavault-cli done frie s01e04      Mark a specific episode as watched
///   mediavault-cli undo frie             Unmark the last watched Frieren episode
///   mediavault-cli ls                    List everything in the library
///   mediavault-cli ls --watching         Only in-progress entries
///   mediavault-cli ls frie               Episode list and progress for Frieren
///   mediavault-cli note frie             Open Frieren notes in $EDITOR
///   mediavault-cli note frie --show      Print existing notes to stdout
#[derive(Parser)]
#[command(
    name = "mediavault-cli",
    bin_name = "mediavault-cli",
    version,
    about = "Terminal media tracker",
    long_about = None,
    after_help = "\
TIPS
  • Run `mediavault-cli` with no args inside your media folder — it auto-detects
    the library from the current directory or your saved config.
  • `mediavault-cli next` with no title resumes the most recently touched entry.
  • Pipe to a player: mpv \"$(mediavault-cli next frie)\"
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
    /// instead of launching a player — useful for: mpv "$(mediavault-cli next frie)"
    ///
    /// Examples:
    ///   mediavault-cli next                  Resume most recent
    ///   mediavault-cli next frie             Play next Frieren episode
    ///   mediavault-cli next matrix           Play The Matrix
    ///   mediavault-cli next frie s01e06      Play a specific episode
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
    ///   mediavault-cli done frie             Mark next Frieren episode watched
    ///   mediavault-cli done frie s01e04      Mark a specific episode watched
    ///   mediavault-cli done matrix           Mark The Matrix watched
    ///   mediavault-cli done frie --all       Mark all Frieren episodes watched
    #[command(visible_alias = "d", visible_alias = "watch")]
    Done {
        /// Partial title to match
        title: String,
        /// Specific episode, e.g. s01e04
        episode: Option<String>,
        /// Mark all episodes watched (shows only)
        #[arg(long)]
        all: bool,
        /// Mark all episodes up to and including this one, e.g. s01e06 (shows only)
        #[arg(long, value_name = "EPISODE", conflicts_with = "all")]
        through: Option<String>,
        /// Mark all episodes in a season as watched, e.g. --season 2 (shows only)
        #[arg(long, value_name = "N", conflicts_with_all = ["all", "through"])]
        season: Option<u32>,
    },

    /// Unmark the last watched episode or movie.
    ///
    /// Examples:
    ///   mediavault-cli undo frie             Unmark last watched Frieren episode
    ///   mediavault-cli undo matrix           Mark The Matrix unwatched
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
    ///   mediavault-cli ls                    List everything
    ///   mediavault-cli ls --watching         Only in-progress entries
    ///   mediavault-cli ls --unwatched        Only unwatched entries
    ///   mediavault-cli ls --watched          Only finished entries
    ///   mediavault-cli ls frie               Episode list for Frieren
    ///   mediavault-cli ls --movies           Movies only
    ///   mediavault-cli ls --shows            Shows only
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
        /// Output as JSON instead of human-readable text
        #[arg(long)]
        json: bool,
    },

    /// Open or print notes for an entry.
    ///
    /// Without --show, opens notes in $EDITOR (falling back to nano/notepad).
    /// With --show, prints existing notes to stdout.
    ///
    /// Examples:
    ///   mediavault-cli note frie             Edit Frieren notes in $EDITOR
    ///   mediavault-cli note frie --show      Print existing notes
    ///   mediavault-cli note matrix           Edit movie notes
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
    ///   mediavault-cli status
    ///   mediavault-cli            (same thing)
    #[command(visible_alias = "s")]
    Status {
        /// Output as JSON instead of human-readable text
        #[arg(long)]
        json: bool,
    },

    /// Query a single field for an entry. Designed for scripting and piping.
    ///
    /// Prints a bare value (no decoration) and exits 0 on success, 1 if not found.
    ///
    /// FIELDS (shows):
    ///   next          Absolute path to next unwatched episode
    ///   next-label    Episode code, e.g. "S01E04" (for status bars)
    ///   watched       "true" or "false"
    ///   progress      "6/24"
    ///   fraction      "0.25" (watched/total as decimal)
    ///   path          Base directory of the show
    ///
    /// FIELDS (movies):
    ///   watched       "true" or "false"
    ///   path          Absolute path to the video file
    ///
    /// EPISODE PATH (shows only):
    ///   mediavault-cli get frie s01e04 path   Absolute path to that episode file
    ///
    /// Examples:
    ///   mediavault-cli get frie next                   Path to next Frieren episode
    ///   mediavault-cli get frie next-label             Prints "S01E07"
    ///   mediavault-cli get frie progress               Prints "6/24"
    ///   mediavault-cli get frie fraction               Prints "0.25"
    ///   mediavault-cli get matrix watched              Prints "true" or "false"
    ///   mediavault-cli get frie s01e04 path            Path to that specific episode
    ///   waybar: $(mediavault-cli get frie next-label)
    #[command(visible_alias = "g")]
    Get {
        /// Partial title to match
        title: String,
        /// Field to query, or an episode specifier (e.g. s01e04) when combined with a field
        field_or_episode: String,
        /// Field to query when field_or_episode is an episode specifier
        field: Option<String>,
    },

    /// Check if an entry or episode is watched. Exits 0 if watched, 1 if not.
    ///
    /// Designed for use in shell conditionals — prints nothing by default.
    /// Use --verbose to also print "watched" or "unwatched".
    ///
    /// Examples:
    ///   mediavault-cli is-watched matrix                   Exit 0 if watched
    ///   mediavault-cli is-watched frie s01e04              Exit 0 if that episode is watched
    ///   mediavault-cli is-watched matrix || mediavault-cli next matrix  Play if not watched
    ///   mediavault-cli is-watched frie && echo "done!"
    ///   mediavault-cli is-watched matrix --verbose          Also prints "watched" or "unwatched"
    #[command(name = "is-watched", visible_alias = "iw")]
    IsWatched {
        /// Partial title to match
        title: String,
        /// Specific episode to check, e.g. s01e04
        episode: Option<String>,
        /// Print "watched" or "unwatched" to stdout
        #[arg(long, short)]
        verbose: bool,
    },

    /// Fetch subtitles from OpenSubtitles.com for a movie or show.
    ///
    /// For movies, searches and downloads subtitles for the video file.
    /// For shows, fetches subtitles for all episodes that don't already have any.
    /// Use --episode to target a specific episode.
    ///
    /// Requires an OpenSubtitles API key in config.toml.
    ///
    /// Examples:
    ///   mediavault-cli fetch-subs matrix                  Interactive subtitle pick
    ///   mediavault-cli fetch-subs matrix --auto            Download best match
    ///   mediavault-cli fetch-subs matrix --lang en         English only
    ///   mediavault-cli fetch-subs frie --episode s01e04    Specific episode
    ///   mediavault-cli fetch-subs frie --list              List available subs
    #[command(name = "fetch-subs", visible_alias = "fs")]
    FetchSubs {
        /// Partial title to match
        title: String,
        /// Specific episode, e.g. s01e04
        #[arg(long, short)]
        episode: Option<String>,
        /// Language filter (ISO 639-1, e.g. "en", "en,de")
        #[arg(long, short, default_value = "")]
        lang: String,
        /// Only list available subtitles, don't download
        #[arg(long)]
        list: bool,
        /// Auto-select the best match (no interactive prompt)
        #[arg(long)]
        auto: bool,
    },

    /// Show embedded subtitle tracks for an entry.
    ///
    /// For movies, lists all subtitle tracks in the video file.
    /// For shows, lists subtitle tracks per episode with a summary.
    ///
    /// Only MKV files are supported — other containers will show no subtitles.
    ///
    /// Examples:
    ///   mediavault-cli subs matrix           Show subtitle tracks for The Matrix
    ///   mediavault-cli subs frie             Show subtitles per episode for Frieren
    ///   mediavault-cli subs matrix --json    Machine-readable output
    #[command(visible_alias = "sub")]
    Subs {
        /// Partial title to match
        title: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();

    let library = match mediavault_core::resolve_library(cli.library) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!("hint:  run from your media folder, or pass --library <path>");
            std::process::exit(1);
        }
    };

    let entries = mediavault_core::scan_library(&library);

    let result = match cli.command.unwrap_or(Command::Status { json: false }) {
        Command::Status { json } => commands::status::run(&entries, json),
        Command::Next {
            title,
            episode,
            path_only,
        } => commands::next::run(&entries, title.as_deref(), episode.as_deref(), path_only),
        Command::Done {
            title,
            episode,
            all,
            through,
            season,
        } => commands::done::run(
            &entries,
            &title,
            episode.as_deref(),
            all,
            through.as_deref(),
            season,
        ),
        Command::Undo { title, episode } => {
            commands::undo::run(&entries, &title, episode.as_deref())
        }
        Command::Ls {
            title,
            watching,
            unwatched,
            watched,
            movies,
            shows,
            json,
        } => commands::ls::run(
            &entries,
            title.as_deref(),
            watching,
            unwatched,
            watched,
            movies,
            shows,
            json,
        ),
        Command::Get {
            title,
            field_or_episode,
            field,
        } => commands::get::run(&entries, &title, &field_or_episode, field.as_deref()),
        Command::IsWatched {
            title,
            episode,
            verbose,
        } => commands::is_watched::run(&entries, &title, episode.as_deref(), verbose),
        Command::Note { title, show } => commands::note::run(&entries, &title, show),
        Command::FetchSubs {
            title,
            episode,
            lang,
            list,
            auto,
        } => commands::fetch_subs::run(
            &entries,
            &title,
            episode.as_deref(),
            &lang,
            list,
            auto,
        ),
        Command::Subs { title, json } => commands::subs::run(&entries, &title, json),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
