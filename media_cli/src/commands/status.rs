use media_core::MediaEntry;
use crate::output::{Style, entry_display_title, progress_bar};

pub fn run(entries: &[MediaEntry], json: bool) -> Result<(), String> {
    if json {
        return run_json(entries);
    }
    let st = Style::new();

    let total_movies = entries.iter().filter(|e| matches!(e, MediaEntry::Movie(_))).count();
    let total_shows  = entries.iter().filter(|e| matches!(e, MediaEntry::Show(_))).count();

    // ── In-progress shows ─────────────────────────────────────────────────────
    let in_progress: Vec<&MediaEntry> = entries.iter().filter(|e| {
        match e {
            MediaEntry::Show(s) => {
                let w = s.watched_count();
                w > 0 && w < s.episode_count()
            }
            MediaEntry::Movie(_) => false,
        }
    }).collect();

    // ── Recently watched movies ───────────────────────────────────────────────
    let recent_movies: Vec<&MediaEntry> = entries.iter().filter(|e| {
        matches!(e, MediaEntry::Movie(m) if m.state.watched)
    }).collect();

    // ── Unwatched entries ─────────────────────────────────────────────────────
    let unwatched_count = entries.iter().filter(|e| match e {
        MediaEntry::Movie(m) => !m.state.watched,
        MediaEntry::Show(s) => s.watched_count() == 0,
    }).count();

    // Header
    println!();
    println!("  {}  {} movies · {} shows · {} unwatched",
        st.bold("MediaVault"),
        total_movies, total_shows, unwatched_count
    );
    println!();

    if in_progress.is_empty() && recent_movies.is_empty() {
        println!("  {}  nothing in progress", st.dim("–"));
        println!();
        println!("  {}  run `mediavault-cli ls` to browse the library", st.dim("hint"));
        println!();
        return Ok(());
    }

    // In-progress shows
    if !in_progress.is_empty() {
        println!("  {}", st.bold("Watching"));
        println!("  {}", st.dim(&"─".repeat(58)));

        // Find the column width for alignment
        let max_title = in_progress.iter()
            .map(|e| entry_display_title(e).len())
            .max()
            .unwrap_or(20)
            .min(36);

        for entry in &in_progress {
            if let MediaEntry::Show(s) = entry {
                let title = entry_display_title(entry);
                let truncated = truncate(title, max_title);
                let next_label = next_ep_label(s);
                let watched = s.watched_count();
                let total = s.episode_count();
                let bar = progress_bar(watched, total, 8);
                println!(
                    "  {:<width$}  {}  {}",
                    st.bold(&truncated),
                    st.yellow(&next_label),
                    st.dim(&bar),
                    width = max_title
                );
            }
        }
        println!();
        println!(
            "  {}  mediavault-cli next <title>  to resume · mediavault-cli next  to resume most recent",
            st.dim("hint")
        );
        println!();
    }

    Ok(())
}

/// Label for the next unwatched episode of a show.
fn next_ep_label(s: &media_core::models::Show) -> String {
    let next_path = s.bookmarks.next_up.as_ref().or_else(|| None);
    let ep = match next_path {
        Some(np) => s.all_episodes().find(|ep| &ep.relative_path == np),
        None => s.all_episodes().find(|ep| !s.bookmarks.is_watched(&ep.relative_path)),
    };
    match ep {
        Some(e) => {
            if e.episode_num > 0 {
                let code = format!("S{:02}E{:02}", e.season_num, e.episode_num);
                match &e.episode_title {
                    Some(t) if !t.is_empty() => format!("{} · {}", code, truncate(t, 28)),
                    _ => code,
                }
            } else {
                e.title.clone()
            }
        }
        None => "—".into(),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", t)
    }
}

fn run_json(entries: &[MediaEntry]) -> Result<(), String> {
    let in_progress: Vec<serde_json::Value> = entries.iter().filter_map(|e| {
        match e {
            MediaEntry::Show(s) => {
                let w = s.watched_count();
                let total = s.episode_count();
                if w == 0 || w >= total { return None; }
                let title = if !s.metadata.clean_title.is_empty() { &s.metadata.clean_title } else { &s.title };
                let next = s.bookmarks.next_up.as_ref()
                    .and_then(|np| s.all_episodes().find(|ep| &ep.relative_path == np))
                    .or_else(|| s.all_episodes().find(|ep| !s.bookmarks.is_watched(&ep.relative_path)));
                Some(serde_json::json!({
                    "title": title,
                    "type": "show",
                    "watched": w,
                    "total": total,
                    "fraction": w as f64 / total as f64,
                    "next_label": next.map(|ep| {
                        if ep.episode_num > 0 {
                            format!("S{:02}E{:02}", ep.season_num, ep.episode_num)
                        } else { ep.title.clone() }
                    }),
                    "next_path": next.map(|ep| ep.video_path.to_string_lossy().to_string()),
                }))
            }
            MediaEntry::Movie(m) => {
                if !m.state.watched { return None; }
                None // movies aren't "in progress"
            }
        }
    }).collect();

    let summary = serde_json::json!({
        "total_movies": entries.iter().filter(|e| matches!(e, MediaEntry::Movie(_))).count(),
        "total_shows": entries.iter().filter(|e| matches!(e, MediaEntry::Show(_))).count(),
        "in_progress": in_progress,
    });

    println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    Ok(())
}
