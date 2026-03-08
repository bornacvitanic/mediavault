use crate::fuzzy::{match_entry, parse_ep_spec, print_ambiguous, print_not_found, MatchResult};
use mediavault_core::MediaEntry;

/// Query a single field from an entry. Always prints a bare value — no colour,
/// no decoration — so output can be used directly in scripts and status bars.
pub fn run(
    entries: &[MediaEntry],
    query: &str,
    field_or_episode: &str,
    field: Option<&str>,
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

    // Determine if field_or_episode is an episode spec (e.g. "s01e04") or a field name.
    // If it parses as an episode spec and a field was provided, it's an episode path query.
    if let (Some(ep_spec), Some(field_name)) = (parse_ep_spec(field_or_episode), field) {
        return get_episode_field(entry, ep_spec, field_name);
    }

    // Otherwise field_or_episode is the field name
    get_entry_field(entry, field_or_episode)
}

fn get_entry_field(entry: &MediaEntry, field: &str) -> Result<(), String> {
    match entry {
        MediaEntry::Movie(m) => match field {
            "watched" => {
                println!("{}", m.state.watched);
                Ok(())
            }
            "path" => {
                println!("{}", m.video_path.display());
                Ok(())
            }
            "title" => {
                let t = if !m.metadata.clean_title.is_empty() {
                    &m.metadata.clean_title
                } else {
                    &m.title
                };
                println!("{t}");
                Ok(())
            }
            "year" => match m.metadata.year {
                Some(y) => {
                    println!("{y}");
                    Ok(())
                }
                None => Err("no year metadata for this entry".into()),
            },
            "subs" => {
                if m.subtitles.is_empty() {
                    println!("none");
                } else {
                    for sub in &m.subtitles {
                        println!("{}", sub.display_label());
                    }
                }
                Ok(())
            }
            other => Err(format!(
                "unknown field \"{other}\" for movie\n  \
                 valid fields: watched, path, title, year, subs"
            )),
        },

        MediaEntry::Show(s) => {
            let watched = s.watched_count();
            let total = s.episode_count();

            match field {
                "next" => {
                    let ep = next_episode(s)
                        .ok_or_else(|| "all episodes watched — nothing next".to_string())?;
                    println!("{}", ep.video_path.display());
                    Ok(())
                }
                "next-label" => {
                    let ep = next_episode(s)
                        .ok_or_else(|| "all episodes watched — nothing next".to_string())?;
                    // Bare code only (no title) — keeps it short for status bars
                    if ep.episode_num > 0 {
                        println!("S{:02}E{:02}", ep.season_num, ep.episode_num);
                    } else {
                        println!("{}", ep.title);
                    }
                    Ok(())
                }
                "watched" => {
                    println!("{}", s.is_fully_watched());
                    Ok(())
                }
                "progress" => {
                    println!("{}/{}", watched, total);
                    Ok(())
                }
                "fraction" => {
                    if total == 0 {
                        println!("0.00");
                    } else {
                        println!("{:.2}", watched as f64 / total as f64);
                    }
                    Ok(())
                }
                "path" => {
                    println!("{}", s.base_dir.display());
                    Ok(())
                }
                "title" => {
                    let t = if !s.metadata.clean_title.is_empty() {
                        &s.metadata.clean_title
                    } else {
                        &s.title
                    };
                    println!("{t}");
                    Ok(())
                }
                "subs" => {
                    let has_any = s.all_episodes().any(|ep| !ep.subtitles.is_empty());
                    println!("{has_any}");
                    Ok(())
                }
                other => Err(format!(
                    "unknown field \"{other}\" for show\n  \
                     valid fields: next, next-label, watched, progress, fraction, path, title, subs\n  \
                     episode path: mediavault-cli get <title> <s01e04> path"
                )),
            }
        }
    }
}

fn get_episode_field(
    entry: &MediaEntry,
    (season, ep_num): (u32, u32),
    field: &str,
) -> Result<(), String> {
    let show = match entry {
        MediaEntry::Show(s) => s,
        MediaEntry::Movie(_) => {
            return Err("episode fields only apply to shows".into());
        }
    };

    let ep = show
        .all_episodes()
        .find(|ep| ep.season_num == season && ep.episode_num == ep_num)
        .ok_or_else(|| format!("episode s{:02}e{:02} not found", season, ep_num))?;

    match field {
        "path" => {
            println!("{}", ep.video_path.display());
            Ok(())
        }
        "watched" => {
            println!("{}", show.bookmarks.is_watched(&ep.relative_path));
            Ok(())
        }
        "label" => {
            println!("{}", ep.display_label());
            Ok(())
        }
        other => Err(format!(
            "unknown episode field \"{other}\"\n  valid fields: path, watched, label"
        )),
    }
}

fn next_episode(s: &mediavault_core::models::Show) -> Option<&mediavault_core::models::Episode> {
    let next_rel = s.bookmarks.next_up.as_ref();
    match next_rel {
        Some(np) => s.all_episodes().find(|ep| &ep.relative_path == np),
        None => s
            .all_episodes()
            .find(|ep| !s.bookmarks.is_watched(&ep.relative_path)),
    }
}
