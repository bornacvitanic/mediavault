use mediavault_core::MediaEntry;

pub enum MatchResult<'a> {
    One(&'a MediaEntry),
    Many(Vec<&'a MediaEntry>),
    None,
}

/// Like `match_entry`, but returns the index into `entries` on a unique match.
/// Used by commands that need mutable access to the matched entry.
pub fn match_entry_index(entries: &[MediaEntry], query: &str) -> Result<usize, String> {
    let q = query.to_lowercase();
    let mut scored: Vec<(u8, usize)> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| score(e, &q).map(|s| (s, i)))
        .collect();

    if scored.is_empty() {
        print_not_found(query);
        return Err("no match".into());
    }

    scored.sort_by_key(|(s, _)| *s);
    let best_score = scored[0].0;
    let best: Vec<usize> = scored
        .iter()
        .filter(|(s, _)| *s == best_score)
        .map(|(_, i)| *i)
        .collect();

    if best.len() == 1 {
        Ok(best[0])
    } else {
        let refs: Vec<&MediaEntry> = best.iter().map(|&i| &entries[i]).collect();
        print_ambiguous(query, &refs);
        Err("ambiguous title".into())
    }
}

/// Fuzzy-match a partial query against the library.
///
/// Scoring (first match wins):
///   1. Exact clean title match (case-insensitive)
///   2. Clean title starts with query
///   3. Any word in the clean title starts with query
///   4. Clean title contains query as substring
pub fn match_entry<'a>(entries: &'a [MediaEntry], query: &str) -> MatchResult<'a> {
    let q = query.to_lowercase();

    // Score each entry; keep only those that match at all.
    let mut scored: Vec<(u8, &MediaEntry)> = entries
        .iter()
        .filter_map(|e| score(e, &q).map(|s| (s, e)))
        .collect();

    if scored.is_empty() {
        return MatchResult::None;
    }

    // Sort by score descending (lower = better match)
    scored.sort_by_key(|(s, _)| *s);

    let best_score = scored[0].0;
    let best: Vec<&MediaEntry> = scored
        .iter()
        .filter(|(s, _)| *s == best_score)
        .map(|(_, e)| *e)
        .collect();

    if best.len() == 1 {
        MatchResult::One(best[0])
    } else {
        MatchResult::Many(best)
    }
}

fn score(entry: &MediaEntry, q: &str) -> Option<u8> {
    let meta = entry.metadata();
    let clean = meta.clean_title.to_lowercase();
    let raw = entry.title().to_lowercase();

    // Try both clean and raw title
    for title in [&clean, &raw] {
        if title == q {
            return Some(0);
        }
        if title.starts_with(q) {
            return Some(1);
        }
        if title.split_whitespace().any(|w| w.starts_with(q)) {
            return Some(2);
        }
        if title.contains(q) {
            return Some(3);
        }
    }
    None
}

/// Parse an episode specifier like "s01e04", "s1e4", "S01E04" into (season, episode).
pub fn parse_ep_spec(s: &str) -> Option<(u32, u32)> {
    let lo = s.to_lowercase();
    // Expect: s<digits>e<digits>
    if !lo.starts_with('s') {
        return None;
    }
    let rest = &lo[1..];
    let e_pos = rest.find('e')?;
    let season: u32 = rest[..e_pos].parse().ok()?;
    let ep: u32 = rest[e_pos + 1..].parse().ok()?;
    Some((season, ep))
}

/// Print a "did you mean?" style ambiguity error listing candidates.
pub fn print_ambiguous(query: &str, candidates: &[&MediaEntry]) {
    eprintln!(
        "  \"{}\" matches {} entries — be more specific:\n",
        query,
        candidates.len()
    );
    for e in candidates {
        let meta = e.metadata();
        let title = if meta.clean_title.is_empty() {
            e.title()
        } else {
            &meta.clean_title
        };
        eprintln!("    {}", title);
    }
}

/// Print a "not found" error with a hint.
pub fn print_not_found(query: &str) {
    eprintln!("  no entry matches \"{}\"", query);
    eprintln!("  hint: run `mediavault-cli ls` to see all titles, or try a shorter search term");
}

#[cfg(test)]
mod tests {
    use super::*;
    use mediavault_core::{MediaEntry, MediaMetadata, Movie, MovieState};
    use std::path::PathBuf;

    fn make_movie(title: &str, clean_title: &str) -> MediaEntry {
        MediaEntry::Movie(Movie {
            title: title.to_string(),
            base_dir: PathBuf::from("."),
            video_path: PathBuf::from(format!("{title}.mkv")),
            video_mtime: None,
            state: MovieState::default(),
            poster_path: PathBuf::from("poster.jpg"),
            metadata: MediaMetadata {
                clean_title: clean_title.to_string(),
                ..Default::default()
            },
            subtitles: Vec::new(),
            external_subs: Vec::new(),
        })
    }

    // ── parse_ep_spec ────────────────────────────────────────────────────────

    #[test]
    fn parse_ep_spec_standard() {
        assert_eq!(parse_ep_spec("s01e04"), Some((1, 4)));
    }

    #[test]
    fn parse_ep_spec_uppercase() {
        assert_eq!(parse_ep_spec("S01E04"), Some((1, 4)));
    }

    #[test]
    fn parse_ep_spec_short() {
        assert_eq!(parse_ep_spec("s1e4"), Some((1, 4)));
    }

    #[test]
    fn parse_ep_spec_mixed_case() {
        assert_eq!(parse_ep_spec("S3e12"), Some((3, 12)));
    }

    #[test]
    fn parse_ep_spec_invalid_no_s() {
        assert_eq!(parse_ep_spec("e04"), None);
    }

    #[test]
    fn parse_ep_spec_invalid_no_e() {
        assert_eq!(parse_ep_spec("s01"), None);
    }

    #[test]
    fn parse_ep_spec_invalid_garbage() {
        assert_eq!(parse_ep_spec("hello"), None);
    }

    #[test]
    fn parse_ep_spec_empty() {
        assert_eq!(parse_ep_spec(""), None);
    }

    // ── match_entry ──────────────────────────────────────────────────────────

    #[test]
    fn match_entry_exact() {
        let entries = vec![
            make_movie("Tron.Legacy.2010", "Tron Legacy"),
            make_movie("Tron.1982", "Tron"),
        ];
        match match_entry(&entries, "tron") {
            MatchResult::One(e) => assert_eq!(e.metadata().clean_title, "Tron"),
            other => panic!("Expected One, got {:?}", matches!(other, MatchResult::None)),
        }
    }

    #[test]
    fn match_entry_starts_with() {
        let entries = vec![
            make_movie("Blade.Runner.2049", "Blade Runner 2049"),
            make_movie("The.Matrix", "The Matrix"),
        ];
        match match_entry(&entries, "blade") {
            MatchResult::One(e) => assert_eq!(e.metadata().clean_title, "Blade Runner 2049"),
            _ => panic!("Expected One"),
        }
    }

    #[test]
    fn match_entry_word_starts() {
        let entries = vec![
            make_movie("The.Matrix", "The Matrix"),
            make_movie("Amadeus", "Amadeus"),
        ];
        match match_entry(&entries, "mat") {
            MatchResult::One(e) => assert_eq!(e.metadata().clean_title, "The Matrix"),
            _ => panic!("Expected One"),
        }
    }

    #[test]
    fn match_entry_contains() {
        let entries = vec![make_movie("Amadeus.1984", "Amadeus")];
        match match_entry(&entries, "adeu") {
            MatchResult::One(e) => assert_eq!(e.metadata().clean_title, "Amadeus"),
            _ => panic!("Expected One"),
        }
    }

    #[test]
    fn match_entry_none() {
        let entries = vec![make_movie("Tron", "Tron")];
        assert!(matches!(match_entry(&entries, "zzz"), MatchResult::None));
    }

    #[test]
    fn match_entry_ambiguous() {
        let entries = vec![
            make_movie("Tron.Legacy", "Tron Legacy"),
            make_movie("Tron.Uprising", "Tron Uprising"),
        ];
        // "tron" starts-with matches both equally
        match match_entry(&entries, "tron") {
            MatchResult::Many(v) => assert_eq!(v.len(), 2),
            _ => panic!("Expected Many"),
        }
    }
}
