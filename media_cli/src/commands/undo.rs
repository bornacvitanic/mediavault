use media_core::{MediaEntry, save_movie_state, save_show_bookmarks};
use crate::fuzzy::{match_entry, parse_ep_spec, print_ambiguous, print_not_found, MatchResult};
use crate::output::{Style, entry_display_title};

pub fn run(
    entries: &[MediaEntry],
    query: &str,
    episode: Option<&str>,
) -> Result<(), String> {
    let st = Style::new();

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

    match entry {
        MediaEntry::Movie(m) => {
            if !m.state.watched {
                println!("  {} is not marked as watched", entry_display_title(entry));
                return Ok(());
            }
            let mut state = m.state.clone();
            state.watched = false;
            // Remove the most recent watch history entry
            state.watch_history.pop();
            save_movie_state(&m.video_path, &state)
                .map_err(|e| format!("failed to save: {e}"))?;
            println!("  {} {} unmarked as watched", st.yellow("↩"), entry_display_title(entry));
        }
        MediaEntry::Show(s) => {
            let mut bookmarks = s.bookmarks.clone();

            let ep_rel = if let Some(spec) = episode {
                // Specific episode requested
                let (season, ep_num) = parse_ep_spec(spec)
                    .ok_or_else(|| format!("couldn't parse \"{spec}\" — use format s01e04"))?;
                s.all_episodes()
                    .find(|ep| ep.season_num == season && ep.episode_num == ep_num)
                    .map(|ep| ep.relative_path.clone())
                    .ok_or_else(|| format!("episode s{:02}e{:02} not found", season, ep_num))?
            } else {
                // Last watched episode — walk the list in order and take the
                // latest one that's marked watched.
                s.all_episodes()
                    .filter(|ep| bookmarks.is_watched(&ep.relative_path))
                    .last()
                    .map(|ep| ep.relative_path.clone())
                    .ok_or_else(|| {
                        format!("no watched episodes found for {}", entry_display_title(entry))
                    })?
            };

            let ep = s.all_episodes()
                .find(|ep| ep.relative_path == ep_rel)
                .ok_or("episode not found")?;

            if !bookmarks.is_watched(&ep_rel) {
                println!("  {} is not marked as watched", ep.display_label());
                return Ok(());
            }

            bookmarks.watched_episodes.retain(|p| p != &ep_rel);
            // Rewind next_up to this episode
            bookmarks.next_up = Some(ep_rel.clone());

            save_show_bookmarks(&s.base_dir, &bookmarks)
                .map_err(|e| format!("failed to save: {e}"))?;

            println!("  {} {} unmarked as watched", st.yellow("↩"), ep.display_label());
            println!("  {} next up reset to {}", st.dim("→"), ep.display_label());
        }
    }

    Ok(())
}
