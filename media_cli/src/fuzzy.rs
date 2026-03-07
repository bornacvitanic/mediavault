use media_core::MediaEntry;

pub enum MatchResult<'a> {
    One(&'a MediaEntry),
    Many(Vec<&'a MediaEntry>),
    None,
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
        if title == q                         { return Some(0); }
        if title.starts_with(q)               { return Some(1); }
        if title.split_whitespace().any(|w| w.starts_with(q)) { return Some(2); }
        if title.contains(q)                  { return Some(3); }
    }
    None
}

/// Parse an episode specifier like "s01e04", "s1e4", "S01E04" into (season, episode).
pub fn parse_ep_spec(s: &str) -> Option<(u32, u32)> {
    let lo = s.to_lowercase();
    // Expect: s<digits>e<digits>
    if !lo.starts_with('s') { return None; }
    let rest = &lo[1..];
    let e_pos = rest.find('e')?;
    let season: u32 = rest[..e_pos].parse().ok()?;
    let ep: u32 = rest[e_pos + 1..].parse().ok()?;
    Some((season, ep))
}

/// Print a "did you mean?" style ambiguity error listing candidates.
pub fn print_ambiguous(query: &str, candidates: &[&MediaEntry]) {
    eprintln!("  \"{}\" matches {} entries — be more specific:\n", query, candidates.len());
    for e in candidates {
        let meta = e.metadata();
        let title = if meta.clean_title.is_empty() { e.title() } else { &meta.clean_title };
        eprintln!("    {}", title);
    }
}

/// Print a "not found" error with a hint.
pub fn print_not_found(query: &str) {
    eprintln!("  no entry matches \"{}\"", query);
    eprintln!("  hint: run `mediavault-cli ls` to see all titles, or try a shorter search term");
}
