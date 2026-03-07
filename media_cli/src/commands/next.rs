use crate::fuzzy::{match_entry, parse_ep_spec, print_ambiguous, print_not_found, MatchResult};
use crate::output::{entry_display_title, Style};
use crate::player::open_or_print;
use chrono::Utc;
use media_core::models::WatchEvent;
use media_core::{save_movie_state, save_show_bookmarks, MediaEntry};

pub fn run(
    entries: &[MediaEntry],
    title: Option<&str>,
    episode: Option<&str>,
    path_only: bool,
) -> Result<(), String> {
    let st = Style::new();

    let entry = match title {
        Some(q) => match match_entry(entries, q) {
            MatchResult::One(e) => e,
            MatchResult::Many(candidates) => {
                print_ambiguous(q, &candidates);
                return Err("ambiguous title".into());
            }
            MatchResult::None => {
                print_not_found(q);
                return Err("no match".into());
            }
        },
        // No title — resume most recently touched in-progress entry
        None => match most_recent_in_progress(entries) {
            Some(e) => e,
            None => {
                eprintln!("  nothing in progress");
                eprintln!("  hint: specify a title, or use `mediavault-cli ls` to browse");
                return Ok(());
            }
        },
    };

    match entry {
        MediaEntry::Movie(m) => {
            let title_str = entry_display_title(entry);
            println!("  {} {}", st.bold("Playing"), st.cyan(title_str));
            open_or_print(&m.video_path, path_only)?;
            // Auto-mark watched
            if !m.state.watched {
                // Re-scan to get mutable — we operate on a fresh mutable copy via sidecar
                let mut state = m.state.clone();
                state.watched = true;
                state.watch_history.push(WatchEvent {
                    watched_at: Utc::now(),
                    note: None,
                });
                save_movie_state(&m.video_path, &state)
                    .map_err(|e| format!("failed to save watch state: {e}"))?;
                println!("  {} marked as watched", st.green("✓"));
            }
        }
        MediaEntry::Show(s) => {
            // Resolve which episode to play
            let ep_rel = if let Some(spec) = episode {
                let (season, ep_num) = parse_ep_spec(spec).ok_or_else(|| {
                    format!("couldn't parse episode \"{spec}\" — use format s01e04")
                })?;
                s.all_episodes()
                    .find(|ep| ep.season_num == season && ep.episode_num == ep_num)
                    .map(|ep| ep.relative_path.clone())
                    .ok_or_else(|| format!("episode s{:02}e{:02} not found", season, ep_num))?
            } else {
                // next_up pointer or first unwatched
                s.bookmarks
                    .next_up
                    .clone()
                    .or_else(|| {
                        s.all_episodes()
                            .find(|ep| !s.bookmarks.is_watched(&ep.relative_path))
                            .map(|ep| ep.relative_path.clone())
                    })
                    .ok_or_else(|| {
                        format!(
                            "all episodes of {} are watched — use `mediavault-cli undo` to rewind",
                            entry_display_title(entry)
                        )
                    })?
            };

            let ep = s
                .all_episodes()
                .find(|ep| ep.relative_path == ep_rel)
                .ok_or_else(|| "episode not found in library".to_string())?;

            let title_str = entry_display_title(entry);
            println!(
                "  {} {}  —  {}",
                st.bold("Playing"),
                st.cyan(title_str),
                ep.display_label()
            );
            open_or_print(&ep.video_path, path_only)?;

            // Auto-mark watched and advance next_up
            let following = s
                .all_episodes()
                .skip_while(|e| e.relative_path != ep_rel)
                .nth(1)
                .map(|e| e.relative_path.clone());
            let mut bookmarks = s.bookmarks.clone();
            bookmarks.mark_watched(&ep_rel, following.as_deref());
            save_show_bookmarks(&s.base_dir, &bookmarks)
                .map_err(|e| format!("failed to save bookmarks: {e}"))?;
            println!(
                "  {} {} marked as watched",
                st.green("✓"),
                ep.display_label()
            );
            if let Some(ref next) = following {
                if let Some(nep) = s.all_episodes().find(|e| &e.relative_path == next) {
                    println!("  {} next up: {}", st.dim("→"), nep.display_label());
                }
            }
        }
    }

    Ok(())
}

/// The most recently touched in-progress show, by last watch event mtime.
fn most_recent_in_progress(entries: &[MediaEntry]) -> Option<&MediaEntry> {
    entries
        .iter()
        .filter(|e| match e {
            MediaEntry::Show(s) => {
                let w = s.watched_count();
                w > 0 && w < s.episode_count()
            }
            MediaEntry::Movie(_) => false,
        })
        .max_by_key(|e| match e {
            MediaEntry::Show(s) => s
                .all_episodes()
                .filter(|ep| s.bookmarks.is_watched(&ep.relative_path))
                .filter_map(|ep| ep.video_mtime)
                .max(),
            _ => None,
        })
}
