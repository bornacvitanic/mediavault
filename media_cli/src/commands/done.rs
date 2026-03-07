use crate::fuzzy::{match_entry, parse_ep_spec, print_ambiguous, print_not_found, MatchResult};
use crate::output::{entry_display_title, Style};
use chrono::Utc;
use media_core::models::WatchEvent;
use media_core::{save_movie_state, save_show_bookmarks, MediaEntry};

pub fn run(
    entries: &[MediaEntry],
    query: &str,
    episode: Option<&str>,
    all: bool,
    through: Option<&str>,
    season: Option<u32>,
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
            if m.state.watched {
                println!("  {} already marked as watched", entry_display_title(entry));
                println!("  hint: use `mediavault-cli undo {}` to unmark", query);
                return Ok(());
            }
            let mut state = m.state.clone();
            state.watched = true;
            state.watch_history.push(WatchEvent {
                watched_at: Utc::now(),
                note: None,
            });
            save_movie_state(&m.video_path, &state).map_err(|e| format!("failed to save: {e}"))?;
            println!(
                "  {} {} marked as watched",
                st.green("✓"),
                entry_display_title(entry)
            );
        }
        MediaEntry::Show(s) => {
            let mut bookmarks = s.bookmarks.clone();

            // ── --season: mark all episodes in a given season ─────────────────
            if let Some(season_num) = season {
                let eps: Vec<String> = s
                    .all_episodes()
                    .filter(|ep| ep.season_num == season_num)
                    .map(|ep| ep.relative_path.clone())
                    .collect();
                if eps.is_empty() {
                    return Err(format!(
                        "season {} not found in {}",
                        season_num,
                        entry_display_title(entry)
                    ));
                }
                let count = eps.len();
                for rel in &eps {
                    bookmarks.mark_watched(rel, None);
                }
                // Advance next_up to first episode of next season
                let next_after = s
                    .all_episodes()
                    .find(|ep| ep.season_num > season_num)
                    .map(|ep| ep.relative_path.clone());
                bookmarks.next_up = next_after;
                save_show_bookmarks(&s.base_dir, &bookmarks)
                    .map_err(|e| format!("failed to save: {e}"))?;
                println!(
                    "  {} season {} — all {} episodes marked as watched",
                    st.green("✓"),
                    season_num,
                    count
                );
                return Ok(());
            }

            // ── --through: mark all episodes up to and including a target ─────
            if let Some(spec) = through {
                let (target_season, target_ep) = parse_ep_spec(spec)
                    .ok_or_else(|| format!("couldn't parse '{spec}' — use format s01e06"))?;
                let mut marked = 0usize;
                let mut found = false;
                let mut following: Option<String> = None;
                let mut past_target = false;
                for ep in s.all_episodes() {
                    if past_target {
                        following = Some(ep.relative_path.clone());
                        break;
                    }
                    if !bookmarks.is_watched(&ep.relative_path) {
                        bookmarks.mark_watched(&ep.relative_path, None);
                        marked += 1;
                    }
                    if ep.season_num == target_season && ep.episode_num == target_ep {
                        found = true;
                        past_target = true;
                    }
                }
                if !found {
                    return Err(format!(
                        "episode s{:02}e{:02} not found",
                        target_season, target_ep
                    ));
                }
                bookmarks.next_up = following;
                save_show_bookmarks(&s.base_dir, &bookmarks)
                    .map_err(|e| format!("failed to save: {e}"))?;
                println!(
                    "  {} {} episodes marked as watched (through s{:02}e{:02})",
                    st.green("✓"),
                    marked,
                    target_season,
                    target_ep
                );
                return Ok(());
            }

            if all {
                // Mark every episode watched and clear next_up
                let count = s.episode_count();
                for ep in s.all_episodes() {
                    bookmarks.mark_watched(&ep.relative_path, None);
                }
                bookmarks.next_up = None;
                save_show_bookmarks(&s.base_dir, &bookmarks)
                    .map_err(|e| format!("failed to save: {e}"))?;
                println!(
                    "  {} all {} episodes of {} marked as watched",
                    st.green("✓"),
                    count,
                    entry_display_title(entry)
                );
                return Ok(());
            }

            // Specific episode or next unwatched
            let ep_rel = if let Some(spec) = episode {
                let (season, ep_num) = parse_ep_spec(spec)
                    .ok_or_else(|| format!("couldn't parse \"{spec}\" — use format s01e04"))?;
                s.all_episodes()
                    .find(|ep| ep.season_num == season && ep.episode_num == ep_num)
                    .map(|ep| ep.relative_path.clone())
                    .ok_or_else(|| format!("episode s{:02}e{:02} not found", season, ep_num))?
            } else {
                // Next unwatched
                bookmarks.next_up.clone()
                    .or_else(|| {
                        s.all_episodes()
                            .find(|ep| !bookmarks.is_watched(&ep.relative_path))
                            .map(|ep| ep.relative_path.clone())
                    })
                    .ok_or_else(|| {
                        format!("all episodes are already watched — use `mediavault-cli undo {}` to rewind", query)
                    })?
            };

            let ep = s
                .all_episodes()
                .find(|ep| ep.relative_path == ep_rel)
                .ok_or("episode not found")?;

            if bookmarks.is_watched(&ep_rel) {
                println!("  {} already watched", ep.display_label());
                return Ok(());
            }

            let following = s
                .all_episodes()
                .skip_while(|e| e.relative_path != ep_rel)
                .nth(1)
                .map(|e| e.relative_path.clone());

            bookmarks.mark_watched(&ep_rel, following.as_deref());
            save_show_bookmarks(&s.base_dir, &bookmarks)
                .map_err(|e| format!("failed to save: {e}"))?;

            println!(
                "  {} {} marked as watched",
                st.green("✓"),
                ep.display_label()
            );

            // Show remaining count
            let new_watched = bookmarks.watched_episodes.len();
            let total = s.episode_count();
            let remaining = total.saturating_sub(new_watched);
            if remaining == 0 {
                println!("  {} show complete!", st.green("★"));
            } else {
                println!("  {} {} episodes remaining", st.dim("→"), remaining);
                if let Some(ref next_rel) = following {
                    if let Some(nep) = s.all_episodes().find(|e| &e.relative_path == next_rel) {
                        println!("  {} next up: {}", st.dim("→"), nep.display_label());
                    }
                }
            }
        }
    }

    Ok(())
}
