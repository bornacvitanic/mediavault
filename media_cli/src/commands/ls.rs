use media_core::MediaEntry;
use crate::fuzzy::{match_entry, print_ambiguous, print_not_found, MatchResult};
use crate::output::{Style, entry_summary_line, entry_display_title, episode_line, progress_bar};

pub fn run(
    entries: &[MediaEntry],
    title: Option<&str>,
    watching: bool,
    unwatched: bool,
    watched: bool,
    movies: bool,
    shows: bool,
) -> Result<(), String> {
    let st = Style::new();

    // ── Detail view for a specific entry ─────────────────────────────────────
    if let Some(q) = title {
        let entry = match match_entry(entries, q) {
            MatchResult::One(e) => e,
            MatchResult::Many(candidates) => {
                print_ambiguous(q, &candidates);
                return Err("ambiguous title".into());
            }
            MatchResult::None => {
                print_not_found(q);
                return Err("no match".into());
            }
        };
        return show_detail(entry, &st);
    }

    // ── Full library list ─────────────────────────────────────────────────────
    let filtered: Vec<&MediaEntry> = entries.iter().filter(|e| {
        // Kind filter
        if movies && matches!(e, MediaEntry::Show(_)) { return false; }
        if shows  && matches!(e, MediaEntry::Movie(_)) { return false; }
        // Watch status filter
        if watching {
            return match e {
                MediaEntry::Show(s) => {
                    let w = s.watched_count();
                    w > 0 && w < s.episode_count()
                }
                MediaEntry::Movie(_) => false,
            };
        }
        if unwatched {
            return match e {
                MediaEntry::Movie(m) => !m.state.watched,
                MediaEntry::Show(s) => s.watched_count() == 0,
            };
        }
        if watched {
            return match e {
                MediaEntry::Movie(m) => m.state.watched,
                MediaEntry::Show(s) => {
                    let total = s.episode_count();
                    total > 0 && s.watched_count() == total
                }
            };
        }
        true
    }).collect();

    if filtered.is_empty() {
        println!("  {}", st.dim("no entries match"));
        return Ok(());
    }

    println!();
    let label = if watching { "Watching" }
        else if unwatched { "Unwatched" }
        else if watched { "Watched" }
        else { "Library" };

    println!("  {}  ({} entries)", st.bold(label), filtered.len());
    println!("  {}", st.dim(&"─".repeat(60)));

    for entry in &filtered {
        println!("  {}", entry_summary_line(entry, &st));
    }
    println!();

    Ok(())
}

fn show_detail(entry: &MediaEntry, st: &Style) -> Result<(), String> {
    println!();
    match entry {
        MediaEntry::Movie(m) => {
            let title = if !m.metadata.clean_title.is_empty() { &m.metadata.clean_title } else { &m.title };
            println!("  {}", st.bold(title));
            if let Some(y) = m.metadata.year {
                println!("  {}", st.dim(&format!("{}", y)));
            }
            let tags = m.metadata.tags();
            if !tags.is_empty() {
                println!("  {}", st.dim(&tags.join("  ")));
            }
            println!();
            let status = if m.state.watched {
                st.green("✓ Watched")
            } else {
                st.dim("○ Unwatched")
            };
            println!("  {}", status);
            if !m.state.watch_history.is_empty() {
                println!();
                println!("  {}", st.dim("Watch history"));
                for event in m.state.watch_history.iter().rev().take(5) {
                    println!("  {}  {}", st.dim("·"),
                        event.watched_at.format("%Y-%m-%d %H:%M"));
                }
            }
        }
        MediaEntry::Show(s) => {
            let title = if !s.metadata.clean_title.is_empty() { &s.metadata.clean_title } else { &s.title };
            let watched = s.watched_count();
            let total = s.episode_count();
            println!("  {}", st.bold(title));
            let tags = s.metadata.tags();
            if !tags.is_empty() {
                println!("  {}", st.dim(&tags.join("  ")));
            }
            println!();
            println!("  {}", progress_bar(watched, total, 20));
            println!();

            // Next up
            let next_rel = s.bookmarks.next_up.as_ref().or_else(|| None);
            let next_rel = next_rel.cloned().or_else(|| {
                s.all_episodes()
                    .find(|ep| !s.bookmarks.is_watched(&ep.relative_path))
                    .map(|ep| ep.relative_path.clone())
            });

            // Group by season
            let mut current_season = u32::MAX;
            for ep in s.all_episodes() {
                if ep.season_num != current_season {
                    current_season = ep.season_num;
                    if ep.season_num > 0 {
                        println!("  {}", st.dim(&format!("Season {}", ep.season_num)));
                    }
                }
                let is_watched = s.bookmarks.is_watched(&ep.relative_path);
                let is_next = next_rel.as_deref() == Some(ep.relative_path.as_str());
                println!("{}", episode_line(ep, is_watched, is_next, st));
            }
        }
    }
    println!();
    Ok(())
}
