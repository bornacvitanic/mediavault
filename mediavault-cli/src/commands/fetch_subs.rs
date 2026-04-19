use crate::fuzzy::{match_entry_index, parse_ep_spec};
use crate::output::Style;
use mediavault_core::opensubtitles::{download_subtitle, search_subtitles, SubtitleResult};
use mediavault_core::tmdb::load_config;
use mediavault_core::MediaEntry;

pub fn run(
    entries: &mut [MediaEntry],
    query: &str,
    episode: Option<&str>,
    language: &str,
    list_only: bool,
    auto_select: bool,
) -> Result<(), String> {
    let config = load_config();
    if config.opensubtitles_api_key.is_empty() {
        return Err(
            "OpenSubtitles API key not configured.\n  \
             Get a free key at https://www.opensubtitles.com/consumers\n  \
             Then set it in your config.toml (opensubtitles_api_key = \"...\")"
                .into(),
        );
    }
    let api_key = &config.opensubtitles_api_key;

    let idx = match_entry_index(entries, query)?;
    // Load subtitles so we can check which episodes already have subs.
    mediavault_core::load_entry_subtitles(&mut entries[idx]);
    let entry = &entries[idx];

    let st = Style::new();
    let meta = entry.metadata();
    let clean_title = if !meta.clean_title.is_empty() {
        &meta.clean_title
    } else {
        entry.title()
    };

    match entry {
        MediaEntry::Movie(m) => {
            println!();
            println!("  {}", st.bold(clean_title));
            fetch_for_video(
                api_key,
                &m.video_path,
                clean_title,
                meta.year,
                None,
                None,
                language,
                list_only,
                auto_select,
                &st,
            )
        }
        MediaEntry::Show(s) => {
            println!();
            println!("  {}", st.bold(clean_title));

            if let Some(ep_spec) = episode {
                // Fetch for a specific episode
                let (sn, en) = parse_ep_spec(ep_spec)
                    .ok_or_else(|| format!("invalid episode specifier: {ep_spec}"))?;
                let ep = s
                    .all_episodes()
                    .find(|ep| ep.season_num == sn && ep.episode_num == en)
                    .ok_or_else(|| format!("episode S{sn:02}E{en:02} not found"))?;
                println!("  {}", st.dim(&ep.display_label()));
                fetch_for_video(
                    api_key,
                    &ep.video_path,
                    clean_title,
                    meta.year,
                    Some(sn),
                    Some(en),
                    language,
                    list_only,
                    auto_select,
                    &st,
                )
            } else {
                // Fetch for all episodes that lack subtitles
                let mut fetched = 0;
                let mut skipped = 0;
                for ep in s.all_episodes() {
                    let has_subs = !ep.subtitles.is_empty() || !ep.external_subs.is_empty();
                    if has_subs && !list_only {
                        skipped += 1;
                        continue;
                    }
                    println!();
                    println!("  {}", ep.display_label());
                    match fetch_for_video(
                        api_key,
                        &ep.video_path,
                        clean_title,
                        meta.year,
                        if ep.season_num > 0 {
                            Some(ep.season_num)
                        } else {
                            None
                        },
                        if ep.episode_num > 0 {
                            Some(ep.episode_num)
                        } else {
                            None
                        },
                        language,
                        list_only,
                        auto_select,
                        &st,
                    ) {
                        Ok(()) => fetched += 1,
                        Err(e) => {
                            eprintln!("  {}", st.dim(&format!("error: {e}")));
                        }
                    }
                }
                if skipped > 0 {
                    println!();
                    println!(
                        "  {}",
                        st.dim(&format!("{skipped} episodes already have subtitles (skipped)"))
                    );
                }
                if fetched == 0 && !list_only {
                    println!(
                        "  {}",
                        st.dim("no subtitles downloaded (all episodes already have subs)")
                    );
                }
                println!();
                Ok(())
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn fetch_for_video(
    api_key: &str,
    video_path: &std::path::Path,
    title: &str,
    year: Option<u32>,
    season: Option<u32>,
    episode: Option<u32>,
    language: &str,
    list_only: bool,
    auto_select: bool,
    st: &Style,
) -> Result<(), String> {
    let results = search_subtitles(api_key, video_path, title, year, season, episode, language)?;

    if results.is_empty() {
        println!("  {}", st.dim("no subtitles found"));
        return Ok(());
    }

    if list_only {
        print_results(&results, st);
        return Ok(());
    }

    if auto_select {
        // Download the top result
        let best = &results[0];
        println!(
            "  downloading: {} — {}",
            best.language,
            st.dim(&best.release)
        );
        let path = download_subtitle(api_key, best.file_id, video_path, &best.language)?;
        println!(
            "  {}",
            st.green(&format!(
                "saved: {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ))
        );
        return Ok(());
    }

    // Interactive: show results and let user pick
    print_results(&results, st);
    pick_and_download(api_key, &results, video_path, st)
}

fn print_results(results: &[SubtitleResult], st: &Style) {
    for (i, r) in results.iter().enumerate().take(15) {
        let hi = if r.hearing_impaired { " [HI]" } else { "" };
        println!(
            "  {}  {} — {}{} {}",
            st.bold(&format!("{:>2}", i + 1)),
            r.language.to_uppercase(),
            r.release,
            hi,
            st.dim(&format!("({} downloads)", r.download_count))
        );
    }
    if results.len() > 15 {
        println!("  {} more results not shown", results.len() - 15);
    }
}

fn pick_and_download(
    api_key: &str,
    results: &[SubtitleResult],
    video_path: &std::path::Path,
    st: &Style,
) -> Result<(), String> {
    use std::io::Write;
    let max = results.len().min(15);
    print!("\n  Pick [1-{max}] or Enter to skip: ");
    std::io::stdout().flush().unwrap();

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| format!("read error: {e}"))?;
    let input = input.trim();

    if input.is_empty() {
        return Ok(());
    }

    let choice: usize = input
        .parse()
        .map_err(|_| "invalid number".to_string())?;
    if choice < 1 || choice > max {
        return Err(format!("pick a number between 1 and {max}"));
    }

    let selected = &results[choice - 1];
    println!(
        "  downloading: {} — {}",
        selected.language,
        st.dim(&selected.release)
    );
    let path = download_subtitle(api_key, selected.file_id, video_path, &selected.language)?;
    println!(
        "  {}",
        st.green(&format!(
            "saved: {}",
            path.file_name().unwrap_or_default().to_string_lossy()
        ))
    );
    Ok(())
}
