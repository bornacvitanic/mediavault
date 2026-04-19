use crate::fuzzy::match_entry_index;
use crate::output::Style;
use mediavault_core::MediaEntry;

pub fn run(entries: &mut [MediaEntry], query: &str, json: bool) -> Result<(), String> {
    let idx = match_entry_index(entries, query)?;
    mediavault_core::load_entry_subtitles(&mut entries[idx]);
    let entry = &entries[idx];

    if json {
        return run_json(entry);
    }

    let st = Style::new();

    match entry {
        MediaEntry::Movie(m) => {
            let title = if !m.metadata.clean_title.is_empty() {
                &m.metadata.clean_title
            } else {
                &m.title
            };
            println!();
            println!("  {}", st.bold(title));

            if m.subtitles.is_empty() && m.external_subs.is_empty() {
                println!("  {}", st.dim("no subtitles"));
            } else {
                if !m.subtitles.is_empty() {
                    println!(
                        "  {} embedded track{}",
                        m.subtitles.len(),
                        if m.subtitles.len() == 1 { "" } else { "s" }
                    );
                    for sub in &m.subtitles {
                        let def = if sub.default { " [default]" } else { "" };
                        println!("  {}  {}{}", st.dim("·"), sub.display_label(), st.dim(def));
                    }
                }
                if !m.external_subs.is_empty() {
                    println!(
                        "  {} external file{}",
                        m.external_subs.len(),
                        if m.external_subs.len() == 1 { "" } else { "s" }
                    );
                    for sub in &m.external_subs {
                        println!(
                            "  {}  {} {}",
                            st.dim("·"),
                            sub.display_label(),
                            st.dim(&format!("({})", sub.filename))
                        );
                    }
                }
            }
            println!();
        }
        MediaEntry::Show(s) => {
            let title = if !s.metadata.clean_title.is_empty() {
                &s.metadata.clean_title
            } else {
                &s.title
            };
            println!();
            println!("  {}", st.bold(title));
            println!();

            let mut lang_set: Vec<String> = Vec::new();
            let mut eps_with_subs = 0usize;
            let mut eps_without_subs = 0usize;

            for ep in s.all_episodes() {
                let has_any = !ep.subtitles.is_empty() || !ep.external_subs.is_empty();
                if !has_any {
                    eps_without_subs += 1;
                } else {
                    eps_with_subs += 1;
                    for sub in &ep.subtitles {
                        let lang = sub
                            .language
                            .as_deref()
                            .unwrap_or("und")
                            .to_uppercase();
                        if !lang_set.contains(&lang) {
                            lang_set.push(lang);
                        }
                    }
                    for sub in &ep.external_subs {
                        let lang = sub
                            .language
                            .as_deref()
                            .unwrap_or("und")
                            .to_uppercase();
                        if !lang_set.contains(&lang) {
                            lang_set.push(lang);
                        }
                    }
                }
            }

            if eps_with_subs == 0 {
                println!("  {}", st.dim("no subtitles in any episode"));
            } else {
                println!(
                    "  {}/{} episodes have subtitles",
                    eps_with_subs,
                    eps_with_subs + eps_without_subs,
                );
                println!("  Languages: {}", lang_set.join(", "));
                println!();
                println!("  {}", st.dim(&"─".repeat(50)));

                let mut current_season = u32::MAX;
                for ep in s.all_episodes() {
                    if ep.season_num != current_season && ep.season_num > 0 {
                        current_season = ep.season_num;
                        println!("  {}", st.dim(&format!("Season {}", ep.season_num)));
                    }
                    let label = ep.display_label();
                    let has_any = !ep.subtitles.is_empty() || !ep.external_subs.is_empty();
                    if !has_any {
                        println!("  {}  {}", st.dim("○"), st.dim(&format!("{label}  —  none")));
                    } else {
                        let mut parts: Vec<String> = ep
                            .subtitles
                            .iter()
                            .map(|s| s.display_label())
                            .collect();
                        for ext in &ep.external_subs {
                            parts.push(format!("{} [file]", ext.display_label()));
                        }
                        println!("  {}  {}  —  {}", st.green("✓"), label, parts.join(", "));
                    }
                }
            }
            println!();
        }
    }

    Ok(())
}

fn run_json(entry: &MediaEntry) -> Result<(), String> {
    let value = match entry {
        MediaEntry::Movie(m) => {
            let embedded: Vec<serde_json::Value> = m
                .subtitles
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "source": "embedded",
                        "track_number": s.track_number,
                        "language": s.language,
                        "codec_id": s.codec_id,
                        "name": s.name,
                        "default": s.default,
                        "forced": s.forced,
                    })
                })
                .collect();
            let external: Vec<serde_json::Value> = m
                .external_subs
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "source": "external",
                        "filename": s.filename,
                        "language": s.language,
                        "format": s.format,
                    })
                })
                .collect();
            let all_subs: Vec<serde_json::Value> = embedded.into_iter().chain(external).collect();
            serde_json::json!({
                "type": "movie",
                "title": if !m.metadata.clean_title.is_empty() { &m.metadata.clean_title } else { &m.title },
                "subtitles": all_subs,
            })
        }
        MediaEntry::Show(s) => {
            let episodes: Vec<serde_json::Value> = s
                .all_episodes()
                .map(|ep| {
                    let embedded: Vec<serde_json::Value> = ep
                        .subtitles
                        .iter()
                        .map(|sub| {
                            serde_json::json!({
                                "source": "embedded",
                                "track_number": sub.track_number,
                                "language": sub.language,
                                "codec_id": sub.codec_id,
                                "name": sub.name,
                                "default": sub.default,
                                "forced": sub.forced,
                            })
                        })
                        .collect();
                    let external: Vec<serde_json::Value> = ep
                        .external_subs
                        .iter()
                        .map(|sub| {
                            serde_json::json!({
                                "source": "external",
                                "filename": sub.filename,
                                "language": sub.language,
                                "format": sub.format,
                            })
                        })
                        .collect();
                    let all_subs: Vec<serde_json::Value> = embedded.into_iter().chain(external).collect();
                    serde_json::json!({
                        "label": ep.display_label(),
                        "subtitles": all_subs,
                    })
                })
                .collect();
            serde_json::json!({
                "type": "show",
                "title": if !s.metadata.clean_title.is_empty() { &s.metadata.clean_title } else { &s.title },
                "episodes": episodes,
            })
        }
    };
    println!("{}", serde_json::to_string_pretty(&value).unwrap());
    Ok(())
}
