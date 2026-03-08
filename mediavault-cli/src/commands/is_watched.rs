use crate::fuzzy::{match_entry, parse_ep_spec, print_ambiguous, print_not_found, MatchResult};
use mediavault_core::MediaEntry;

/// Check watch state via exit code. Exits 0 if watched, 1 if not.
/// Prints nothing unless --verbose is set, so it composes cleanly in conditionals.
pub fn run(
    entries: &[MediaEntry],
    query: &str,
    episode: Option<&str>,
    verbose: bool,
) -> Result<(), String> {
    let entry = match match_entry(entries, query) {
        MatchResult::One(e) => e,
        MatchResult::Many(candidates) => {
            print_ambiguous(query, &candidates);
            return Err("ambiguous title".into());
        }
        MatchResult::None => {
            print_not_found(query);
            return Err("no match".into());
        }
    };

    let is_watched = match entry {
        MediaEntry::Movie(m) => {
            if episode.is_some() {
                return Err("movies don't have episodes — omit the episode argument".into());
            }
            m.state.watched
        }
        MediaEntry::Show(s) => match episode {
            Some(spec) => {
                let (season, ep_num) = parse_ep_spec(spec)
                    .ok_or_else(|| format!("couldn't parse \"{spec}\" — use format s01e04"))?;
                let ep = s
                    .all_episodes()
                    .find(|ep| ep.season_num == season && ep.episode_num == ep_num)
                    .ok_or_else(|| format!("episode s{:02}e{:02} not found", season, ep_num))?;
                s.bookmarks.is_watched(&ep.relative_path)
            }
            None => s.is_fully_watched(),
        },
    };

    if verbose {
        println!("{}", if is_watched { "watched" } else { "unwatched" });
    }

    // Exit code is the primary output: 0 = watched, 1 = not watched.
    // Return an error string to trigger the non-zero exit in main().
    if is_watched {
        Ok(())
    } else {
        // Use process::exit directly so we don't print "error:" prefix
        std::process::exit(1);
    }
}
