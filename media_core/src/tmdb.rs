/// TMDB (The Movie Database) poster fetching.
///
/// Requires a free API key from https://www.themoviedb.org/settings/api
/// The key is stored in `%APPDATA%\mediavault\config.toml` on Windows.
///
/// Fetching is intentionally blocking so it can be called from a background
/// thread without pulling in an async runtime.
use std::{fs, path::Path};

use serde::Deserialize;

const TMDB_BASE: &str = "https://api.themoviedb.org/3";
const TMDB_IMAGE_BASE: &str = "https://image.tmdb.org/t/p/w300";

// ── Config ────────────────────────────────────────────────────────────────────

/// Returns the path to the app config file, creating parent directories if
/// needed. On non-Windows platforms falls back to `~/.config/mediavault/`.
pub fn config_path() -> std::io::Result<std::path::PathBuf> {
    let base = std::env::var("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            dirs_next::config_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
        });
    let dir = base.join("mediavault");
    fs::create_dir_all(&dir)?;
    Ok(dir.join("config.toml"))
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppConfig {
    /// TMDB v3 API key. Leave empty to disable poster fetching.
    #[serde(default)]
    pub tmdb_api_key: String,
    /// Whether to fetch and display poster images.
    #[serde(default = "default_true")]
    pub show_posters: bool,
    /// Automatically mark an entry as watched when it is opened in the player.
    #[serde(default = "default_true")]
    pub auto_mark_watched: bool,
    /// Last used library path — shared between GUI and CLI so both tools
    /// can default to the same folder without extra configuration.
    #[serde(default)]
    pub library_path: String,
}

fn default_true() -> bool { true }

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            tmdb_api_key: String::new(),
            show_posters: true,
            auto_mark_watched: true,
            library_path: String::new(),
        }
    }
}

pub fn load_config() -> AppConfig {
    let path = match config_path() {
        Ok(p) => p,
        Err(_) => return AppConfig::default(),
    };
    let raw = match fs::read_to_string(&path) {
        Ok(r) => r,
        Err(_) => return AppConfig::default(),
    };
    toml::from_str(&raw).unwrap_or_default()
}

pub fn save_config(cfg: &AppConfig) -> std::io::Result<()> {
    let path = config_path()?;
    let raw = toml::to_string_pretty(cfg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    let content = format!(
        "# MediaVault configuration\n\
         # Get a free TMDB API key at https://www.themoviedb.org/settings/api\n\n\
         {raw}"
    );
    fs::write(path, content)
}

// ── TMDB API types ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SearchResult {
    results: Vec<SearchHit>,
}

#[derive(Deserialize)]
struct SearchHit {
    #[serde(default)]
    poster_path: Option<String>,
}

// ── Title cleaning ────────────────────────────────────────────────────────────

/// Noise tokens that signal everything from that point onward is release
/// metadata, not part of the actual title. Matched case-insensitively.
const NOISE_TOKENS: &[&str] = &[
    // Resolutions
    "480p", "576p", "720p", "1080p", "1080i", "2160p", "4k", "8k",
    // Sources
    "bluray", "blu-ray", "bdrip", "bdremux", "remux",
    "webrip", "web-rip", "webdl", "web-dl", "web", "hdtv", "dvdrip",
    "dvd", "hdrip", "hdcam", "cam", "scr", "r5",
    // HDR / colour
    "hdr", "hdr10", "dv", "dolbyvision", "hlg", "sdr",
    // Codecs
    "x264", "x265", "h264", "h265", "hevc", "avc", "xvid", "divx",
    "av1", "vp9", "10bit", "8bit",
    // Audio
    "aac", "ac3", "dd5", "dts", "dtshd", "atmos", "truehd", "flac",
    "mp3", "opus", "ddp", "eac3",
    // Languages / subs
    "multi", "dual", "dubbed", "sub", "subbed", "eng", "ita", "fra",
    "ger", "spa", "por", "rus", "jpn", "japanese", "english",
    // Misc release tags
    "proper", "repack", "extended", "theatrical", "unrated",
    "remastered", "retail", "internal", "limited", "batch", "specials",
];

fn is_year(tok: &str) -> bool {
    tok.len() == 4
        && tok.chars().all(|c| c.is_ascii_digit())
        && tok.parse::<u32>().map(|y| (1900..=2100).contains(&y)).unwrap_or(false)
}

/// Returns true if `tok` is a season marker that should truncate the title
/// for TMDB queries: "season", "saison", or bare S-number like "s01"/"s1".
fn is_season_token(tok: &str) -> bool {
    if tok == "season" || tok == "saison" { return true; }
    if tok.len() >= 2 && tok.len() <= 4 && tok.starts_with('s') {
        let rest = &tok[1..];
        if rest.chars().all(|c| c.is_ascii_digit()) && rest.parse::<u32>().is_ok() {
            return true;
        }
    }
    false
}

fn is_size_token(tok: &str) -> bool {
    let t = tok.trim_end_matches("mb").trim_end_matches("gb");
    !t.is_empty() && t.chars().all(|c| c.is_ascii_digit())
}

/// Extract a season number from a raw string, returning (season_number, display_label).
/// Recognises patterns like: S01, S1, Season 1, Season.1, s02, SAISON 2
pub fn extract_season(raw: &str) -> Option<(u32, String)> {
    let spaced = raw.replace(['.', '_'], " ");
    let tokens: Vec<&str> = spaced.split_whitespace().collect();

    for (i, tok) in tokens.iter().enumerate() {
        let lo = tok.to_lowercase();

        // "Season 2" or "Saison 2" — word followed by a number token
        if lo == "season" || lo == "saison" || lo == "s" {
            if let Some(next) = tokens.get(i + 1) {
                let n = next.trim_matches(|c: char| !c.is_ascii_digit());
                if !n.is_empty() {
                    if let Ok(num) = n.parse::<u32>() {
                        return Some((num, format!("S{num}")));
                    }
                }
            }
        }

        // Bare "S01" / "S1" / "s02" token (not followed by E\d — that would be an episode tag)
        if lo.starts_with('s') && lo.len() >= 2 && lo.len() <= 4 {
            let rest = &lo[1..];
            if rest.chars().all(|c| c.is_ascii_digit()) {
                if let Ok(num) = rest.parse::<u32>() {
                    // Make sure the next token is not an episode marker (E01)
                    let next_is_ep = tokens.get(i + 1)
                        .map(|t| t.to_lowercase().starts_with('e') && t.len() <= 4)
                        .unwrap_or(false);
                    if !next_is_ep {
                        return Some((num, format!("S{num}")));
                    }
                }
            }
        }

        // "S01E02" combined token — extract just the season part
        if lo.len() >= 4 {
            if let Some(e_pos) = lo.find('e') {
                let s_part = &lo[..e_pos];
                if s_part.starts_with('s') {
                    let n = &s_part[1..];
                    if n.chars().all(|c| c.is_ascii_digit()) {
                        if let Ok(num) = n.parse::<u32>() {
                            return Some((num, format!("S{num}")));
                        }
                    }
                }
            }
        }
    }
    None
}

/// Strip all `[...]` bracket groups — handles anime-style tags like
/// `[BD][1080p][HEVC 10bit x265][Tenrai-Sensei]`.
fn strip_brackets(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0usize;
    for ch in s.chars() {
        match ch {
            '[' => depth += 1,
            ']' => { depth = depth.saturating_sub(1); }
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}

/// Extract the first 4-digit year found in a raw filename.
pub fn extract_year(raw: &str) -> Option<u32> {
    let spaced = raw.replace(['.', '_'], " ");
    for token in spaced.split_whitespace() {
        let t = token.trim_matches(|c: char| !c.is_alphanumeric());
        if is_year(t) {
            return t.parse().ok();
        }
    }
    None
}

/// Clean a raw filename or folder name into a plain title for TMDB search.
///
/// Handles all common release naming patterns:
///   `Tron.Legacy.2010.2160p.UHD.BluRay.REMUX.DV.P7.HDR.MULTI-BenT`
///   `[UsaBit.com] - Pirates.of.Silicon.Valley.1999.DVDRip.x264-RQQU`
///   `Dr. Stone [Season 3 + Specials] [BD][1080p][HEVC 10bit x265][Batch]`
///   `Apocalypse Hotel (2025) 501 (1080p WEB-DL H264 DDP 2.0 x265)[Cytox]`
///   `Frieren Beyond Journey's End [BD][1080p]...[Tenrai-Sensei]`
pub fn clean_title(raw: &str) -> String {
    // Normalize Unicode lookalike punctuation before any other processing.
    // Some release names use Windows-filename-safe lookalikes, e.g.
    // U+A789 MODIFIER LETTER COLON (꞉) instead of ':' which is illegal in
    // Windows filenames. TMDB will not match these without normalization.
    let normalized: String = raw.chars().map(|c| match c {
        '\u{A789}' | '\u{FE13}' | '\u{FE55}' | '\u{FF1A}' => ':',  // colon lookalikes
        '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}'
        | '\u{FE58}' | '\u{FF0D}' => '-',                            // dash lookalikes
        '\u{2018}' | '\u{2019}' | '\u{FF07}' => '\'',               // apostrophe lookalikes
        '\u{FF01}' => '!',
        '\u{FF1F}' => '?',
        '\u{FF06}' => '&',
        _ => c,
    }).collect();
    let raw = normalized.as_str();
    let s = raw.trim();


    // 1. Strip leading `[domain.com] - ` style site prefix.
    //    Only strip if the bracket content looks like a domain (contains '.').
    let s: &str = if s.starts_with('[') {
        if let Some(close) = s.find(']') {
            let bracket_content = &s[1..close];
            if bracket_content.contains('.') {
                s[close + 1..]
                    .trim_start_matches(|c: char| c == ' ' || c == '-')
                    .trim()
            } else {
                s
            }
        } else {
            s
        }
    } else {
        s
    };

    // 2. Remove all [...] bracket groups (anime release tags, season labels, etc.)
    //    We do this AFTER stripping the leading site tag so we don't lose the title.
    let no_brackets = strip_brackets(s);

    // 3. Replace dot/underscore word separators with spaces.
    let spaced = no_brackets.replace(['.', '_'], " ");

    // 4. Tokenise and truncate at the first noise signal.
    let mut keep: Vec<&str> = Vec::new();
    for token in spaced.split_whitespace() {
        let lower = token.to_lowercase();
        let clean = lower.trim_matches(|c: char| !c.is_alphanumeric());

        if NOISE_TOKENS.contains(&clean)
            || is_year(clean)
            || is_size_token(clean)
            // Dash-prefixed release group names like `-BenT`, `-GalaxyRG`
            || (token.starts_with('-') && token.len() > 1)
            // Season markers: "Season", "Saison", bare "S01"/"S1"
            || is_season_token(clean)
        {
            break;
        }
        keep.push(token);
    }

    if keep.is_empty() {
        keep.push(spaced.split_whitespace().next().unwrap_or(raw));
    }

    // 5. Strip trailing parenthesised year like `(2025)`.
    let mut joined = keep.join(" ");
    if let Some(paren) = joined.rfind('(') {
        let inner = joined[paren..].trim_matches(|c| c == '(' || c == ')' || c == ' ');
        if is_year(inner.trim()) {
            joined = joined[..paren].trim().to_string();
        }
    }

    joined.trim().to_string()
}

// ── TMDB search ───────────────────────────────────────────────────────────────

/// Query TMDB for `query` on `endpoint` ("movie" or "tv"), optionally
/// anchored to `year`. Returns the first result's poster path if any.
fn search_tmdb(
    endpoint: &str,
    query: &str,
    year: Option<u32>,
    api_key: &str,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let year_param = match year {
        Some(y) if endpoint == "movie" => format!("&primary_release_year={y}"),
        Some(y) => format!("&first_air_date_year={y}"),
        None => String::new(),
    };

    let url = format!(
        "{TMDB_BASE}/search/{endpoint}?api_key={api_key}&query={}{year_param}&page=1",
        urlencoding::encode(query)
    );

    let response: SearchResult = ureq::get(&url).call()?.into_json()?;
    Ok(response.results.first().and_then(|h| h.poster_path.clone()))
}

// ── Public fetch function ─────────────────────────────────────────────────────

/// Fetch and cache a poster for `title` into `cache_path`.
///
/// Returns `Ok(true)` if a poster was written, `Ok(false)` if nothing was
/// found after all attempts, `Err` on unrecoverable network/IO failure.
///
/// Search strategy (stops at first hit):
/// For each query variant (full cleaned title → drop last word, up to 3):
///   1. Primary endpoint + year  (most specific, least ambiguous)
///   2. Other endpoint  + year   (handles mis-classified entries)
///   3. Primary endpoint, no year
///   4. Other endpoint,  no year
///
/// Passing the year extracted from the filename to TMDB's year filter is the
/// key fix for wrong-poster issues: "Tron (1982)" and "Tron Legacy (2010)"
/// are unambiguous once the year is included.
pub fn fetch_poster(
    title: &str,
    is_movie: bool,
    api_key: &str,
    cache_path: &Path,
) -> Result<bool, Box<dyn std::error::Error>> {
    if api_key.is_empty() {
        return Ok(false);
    }

    let cleaned = clean_title(title);
    let year = extract_year(title);
    let primary = if is_movie { "movie" } else { "tv" };
    let other = if is_movie { "tv" } else { "movie" };

    let words: Vec<&str> = cleaned.split_whitespace().collect();
    let query_variants: Vec<String> = (1..=words.len())
        .rev()
        .take(3)
        .map(|n| words[..n].join(" "))
        .collect();

    for query in &query_variants {
        if query.is_empty() {
            continue;
        }
        for (endpoint, yr) in &[(primary, year), (other, year), (primary, None), (other, None)] {
            if let Some(poster_path) = search_tmdb(endpoint, query, *yr, api_key)? {
                let image_url = format!("{TMDB_IMAGE_BASE}{poster_path}");
                let mut reader = ureq::get(&image_url).call()?.into_reader();
                let mut buf = Vec::new();
                std::io::Read::read_to_end(&mut reader, &mut buf)?;
                fs::write(cache_path, &buf)?;
                return Ok(true);
            }
        }
    }

    Ok(false)
}

// ── Full metadata extraction ──────────────────────────────────────────────────

/// Split a fused token like `BD1080p` into `["BD", "1080p"]` before scanning.
///
/// Preserves known compound tokens that should not be split:
/// - `x265`, `x264`, `h265`, `h264` — codec identifiers
/// - `S01E02` style — episode markers handled elsewhere
/// - Plain resolutions like `1080p`, `720p` — already clean
///
/// Handles patterns like `BD1080p`, `WEB1080p`, `BD720p` by detecting an
/// alpha prefix followed directly by a digit-then-p/i resolution suffix.
fn expand_token(tok: &str) -> Vec<String> {
    // x265/x264/h265/h264 — keep intact
    if tok.len() == 4 {
        let lo = tok.to_lowercase();
        if (lo.starts_with('x') || lo.starts_with('h'))
            && lo[1..].chars().all(|c| c.is_ascii_digit())
        {
            return vec![tok.to_string()];
        }
    }
    // S01E02 style — keep intact
    if tok.len() >= 4 {
        let lo = tok.to_lowercase();
        let mut chars = lo.chars();
        if chars.next() == Some('s') {
            let rest: String = chars.collect();
            if rest.contains('e') {
                return vec![tok.to_string()];
            }
        }
    }
    // Alpha prefix + digit resolution suffix: BD1080p, WEB720p, etc.
    // Match: one or more letters, then 3-4 digits, then 'p' or 'i'
    let bytes = tok.as_bytes();
    if let Some(digit_start) = bytes.iter().position(|b| b.is_ascii_digit()) {
        if digit_start > 0 {
            let prefix = &tok[..digit_start];
            let suffix = &tok[digit_start..];
            // Check suffix is a resolution: 3-4 digits + p/i
            let suffix_lo = suffix.to_lowercase();
            let digits: String = suffix_lo.chars().take_while(|c| c.is_ascii_digit()).collect();
            let rest: String = suffix_lo.chars().skip_while(|c| c.is_ascii_digit()).collect();
            if (digits.len() == 3 || digits.len() == 4)
                && (rest == "p" || rest == "i" || rest.is_empty())
            {
                let res = format!("{}{}", digits, if rest.is_empty() { "p" } else { &rest });
                return vec![prefix.to_string(), res];
            }
        }
    }
    vec![tok.to_string()]
}

/// Extract all bracket group contents from a string as a flat list of tokens,
/// with fused tokens like `BD1080p` split into their components.
/// e.g. `"Title [BD][1080p][HEVC]"` -> `["BD", "1080p", "HEVC"]`
/// e.g. `"[DB]Title [BD1080p][x265]"` -> `["DB", "BD", "1080p", "x265"]`
fn bracket_tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for ch in s.chars() {
        match ch {
            '[' | '(' => {
                if depth == 0 { current.clear(); }
                depth += 1;
            }
            ']' | ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 && !current.trim().is_empty() {
                    for tok in current.split_whitespace() {
                        let t = tok.replace(['.','_'], " ");
                        for word in t.split_whitespace() {
                            for part in expand_token(word) {
                                out.push(part);
                            }
                        }
                    }
                    current.clear();
                }
            }
            _ if depth > 0 => current.push(ch),
            _ => {}
        }
    }
    out
}

/// Scan a token list and fill in any missing metadata fields.
/// This is the core extraction loop, shared by folder name and episode fallback paths.
fn scan_tokens(
    tokens: impl Iterator<Item = String>,
    year: &mut Option<u32>,
    resolution: &mut Option<String>,
    source: &mut Option<String>,
    hdr: &mut Option<String>,
    codec: &mut Option<String>,
) {
    for token in tokens {
        let lo = token.to_lowercase();
        let clean = lo.trim_matches(|c: char| !c.is_alphanumeric());

        if year.is_none() && is_year(clean) {
            *year = clean.parse().ok();
            continue;
        }
        if resolution.is_none() {
            *resolution = match clean {
                "2160p" | "4k" | "uhd" => Some("4K".into()),
                "1080p" | "1080i" => Some("1080p".into()),
                "720p" => Some("720p".into()),
                "480p" | "576p" => Some("480p".into()),
                _ => None,
            };
            if resolution.is_some() { continue; }
        }
        if source.is_none() {
            *source = match clean {
                "bluray" | "blu-ray" | "bdremux" | "remux" | "bdrip" | "bd" => Some("BluRay".into()),
                "webrip" | "web-rip" => Some("WEBRip".into()),
                "webdl" | "web-dl" | "web" => Some("WEB-DL".into()),
                "hdtv" => Some("HDTV".into()),
                "dvdrip" | "dvd" => Some("DVD".into()),
                "hdrip" => Some("HDRip".into()),
                _ => None,
            };
            if source.is_some() { continue; }
        }
        if hdr.is_none() {
            *hdr = match clean {
                "dv" | "dolbyvision" => Some("DV".into()),
                "hdr10+" => Some("HDR10+".into()),
                "hdr10" => Some("HDR10".into()),
                "hdr" => Some("HDR".into()),
                "hlg" => Some("HLG".into()),
                _ => None,
            };
            if hdr.is_some() { continue; }
        }
        if codec.is_none() {
            *codec = match clean {
                "x265" | "h265" | "hevc" => Some("x265".into()),
                "x264" | "h264" | "avc" => Some("x264".into()),
                "av1" => Some("AV1".into()),
                "xvid" | "divx" => Some("XviD".into()),
                _ => None,
            };
        }
    }
}

/// Normalise Unicode lookalike punctuation (Windows-safe substitutes) to ASCII.
fn normalise_unicode(raw: &str) -> String {
    raw.chars().map(|c| match c {
        '\u{A789}' | '\u{FE13}' | '\u{FE55}' | '\u{FF1A}' => ':',
        '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}'
        | '\u{FE58}' | '\u{FF0D}' => '-',
        '\u{2018}' | '\u{2019}' | '\u{FF07}' => '\'',
        _ => c,
    }).collect()
}

/// Extract all useful metadata from a raw folder/filename.
///
/// Strategy:
/// 1. Scan tokens *outside* brackets (dot/space separated) — catches `1080p` in
///    `Show.1080p.BluRay` style names.
/// 2. Scan tokens *inside* every bracket group — catches `[BD][1080p][HEVC 10bit x265]`
///    style names common in anime releases.
/// 3. Optionally, fall back to a list of episode filename stems when the folder
///    name alone has no tags. Use `extract_metadata_with_episodes` for this.
pub fn extract_metadata(raw: &str) -> crate::models::MediaMetadata {
    extract_metadata_with_episodes(raw, &[])
}

/// Like `extract_metadata` but also consults `episode_stems` (bare filenames
/// without extension) when the folder name yields no tags. Only the tokens that
/// are *common to all* episode stems are used, to avoid picking up per-episode
/// noise like episode numbers.
pub fn extract_metadata_with_episodes(
    raw: &str,
    episode_stems: &[String],
) -> crate::models::MediaMetadata {
    use crate::models::MediaMetadata;

    let normalized = normalise_unicode(raw);

    let mut year: Option<u32> = None;
    let mut resolution: Option<String> = None;
    let mut source: Option<String> = None;
    let mut hdr: Option<String> = None;
    let mut codec: Option<String> = None;

    // Pass 1: tokens outside brackets (dot/underscore → space), with fused
    // token expansion so e.g. `BD1080p` outside brackets also gets split.
    let outside = strip_brackets(&normalized).replace(['.', '_'], " ");
    scan_tokens(
        outside.split_whitespace().flat_map(|s| expand_token(s)),
        &mut year, &mut resolution, &mut source, &mut hdr, &mut codec,
    );

    // Pass 2: tokens inside bracket groups — covers [BD][1080p][HEVC 10bit x265]
    scan_tokens(
        bracket_tokens(&normalized).into_iter(),
        &mut year, &mut resolution, &mut source, &mut hdr, &mut codec,
    );

    // Pass 3: episode filename fallback when folder gave us nothing useful.
    // Find tokens that appear in ALL episode stems (i.e. are release-wide constants).
    if (resolution.is_none() && source.is_none() && codec.is_none())
        && !episode_stems.is_empty()
    {
        // Collect token sets for each episode stem (bracket contents + outer tokens)
        let token_sets: Vec<std::collections::HashSet<String>> = episode_stems
            .iter()
            .map(|stem| {
                let n = normalise_unicode(stem);
                let mut set = std::collections::HashSet::new();
                // outside tokens (with fused expansion)
                for t in strip_brackets(&n).replace(['.','_']," ").split_whitespace() {
                    for part in expand_token(t) {
                        set.insert(part.to_lowercase());
                    }
                }
                // bracket tokens (already expanded inside bracket_tokens)
                for t in bracket_tokens(&n) {
                    set.insert(t.to_lowercase());
                }
                set
            })
            .collect();

        // Intersection: tokens present in every episode
        if let Some(first) = token_sets.first() {
            let common: Vec<String> = first
                .iter()
                .filter(|t| token_sets.iter().all(|s| s.contains(*t)))
                .cloned()
                .collect();

            scan_tokens(
                common.into_iter(),
                &mut year, &mut resolution, &mut source, &mut hdr, &mut codec,
            );
        }
    }

    // Year fallback: raw string scan (catches years inside brackets like `(2025)`)
    if year.is_none() {
        year = extract_year(raw);
    }

    let season = extract_season(raw);

    let base = clean_title(raw);
    let display_title = match &season {
        Some((_n, label)) if !base.to_lowercase().contains(&label.to_lowercase()) => {
            format!("{base} {label}")
        }
        _ => base,
    };

    MediaMetadata {
        clean_title: display_title,
        year,
        resolution,
        source,
        hdr,
        codec,
        season,
    }
}

// ── Episode filename parser ───────────────────────────────────────────────────

/// Parsed episode information extracted from a raw filename stem.
#[derive(Debug, Default)]
pub struct ParsedEpisode {
    pub season_num: u32,
    pub episode_num: u32,
    pub episode_title: Option<String>,
}

/// Parse a raw episode filename stem into structured metadata.
///
/// Supports common release naming conventions:
///   `Delicious In Dungeon - S01E01 - Hot Pot`
///   `Apocalypse Hotel (2025) S01E01 A True Hotel...`
///   `[DiabloTripleA] Dr Stone - S03E01 [D5ACD9A8]`
///   `Frieren Beyond Journey's End - S01E01 - The Journey's End`
///   `[DB]Gurren Lagann_-_08_(Dual Audio_10bit_BD1080p_x265)`
///
/// Strategy:
/// 1. Strip leading [Group] tags and trailing noise (CRC hashes, release tags).
/// 2. Try to match SxxExx pattern → extract season, episode, and optional title.
/// 3. Fall back to a bare 2-3 digit episode number (common in older fansubs).
pub fn parse_episode(raw_stem: &str) -> ParsedEpisode {
    let normalised = normalise_unicode(raw_stem);
    // Replace underscores with spaces for uniform tokenisation.
    let s = normalised.replace('_', " ");

    // Strip leading [Group] tag like `[DB]` or `[DiabloTripleA]`.
    let s = if s.trim_start().starts_with('[') {
        if let Some(close) = s.find(']') {
            s[close + 1..].trim_start_matches(|c: char| c == ' ' || c == '-').to_string()
        } else { s }
    } else { s };

    // Strip trailing CRC hash like `[D5ACD9A8]` (8 hex chars).
    let s = regex_strip_trailing_hash(&s);

    // Strip trailing release tag blocks: (1080p WEB-DL ...) [Cytox] etc.
    // We do this by finding the last SxxExx match and only cleaning after it.
    let s = strip_trailing_release_tags(&s);

    // Find SxxExx (or SxExx) pattern — case insensitive.
    if let Some((season, episode, after)) = find_sxexx(&s) {
        // Everything after the SxxExx marker is a candidate episode title.
        let title = clean_episode_title(after);
        return ParsedEpisode {
            season_num: season,
            episode_num: episode,
            episode_title: if title.is_empty() { None } else { Some(title) },
        };
    }

    // Fallback: bare 2-3 digit episode number after a separator.
    // e.g. `Show - 08`, `Show_-_08_`
    if let Some(ep) = find_bare_episode_number(&s) {
        return ParsedEpisode {
            season_num: 1,
            episode_num: ep,
            episode_title: None,
        };
    }

    ParsedEpisode::default()
}

/// Strip trailing 6-8 character hex CRC hashes like `[D5ACD9A8]`.
fn regex_strip_trailing_hash(s: &str) -> String {
    let trimmed = s.trim_end();
    if trimmed.ends_with(']') {
        if let Some(open) = trimmed.rfind('[') {
            let inner = &trimmed[open + 1..trimmed.len() - 1];
            if inner.len() >= 6 && inner.len() <= 8
                && inner.chars().all(|c| c.is_ascii_hexdigit())
            {
                return trimmed[..open].trim_end().to_string();
            }
        }
    }
    trimmed.to_string()
}

/// Strip trailing release tag blocks after the episode title.
/// Only removes content inside `[...]` or `(...)` that contains known noise
/// (resolution, codec, source, etc.) at the end of the string.
fn strip_trailing_release_tags(s: &str) -> String {
    let noise_re = [
        "1080p","720p","2160p","4k","x265","x264","hevc","h264","avc",
        "bluray","web-dl","webrip","web","bd","hdr","dv","aac","ac3",
        "ddp","flac","10bit","dual audio","dual","japanese","english",
    ];
    let mut result = s.trim().to_string();
    // Repeatedly strip trailing bracket groups that contain noise words.
    loop {
        let trimmed = result.trim_end().to_string();
        let (open_ch, _close_ch) = if trimmed.ends_with(')') { ('(', ')') }
                                   else if trimmed.ends_with(']') { ('[', ']') }
                                   else { break; };
        if let Some(open) = trimmed.rfind(open_ch) {
            let inner = trimmed[open + 1..trimmed.len() - 1].to_lowercase();
            let is_noise = noise_re.iter().any(|n| inner.contains(n));
            if is_noise {
                result = trimmed[..open].trim_end().to_string();
            } else {
                break;
            }
        } else {
            break;
        }
    }
    result
}

/// Find an SxxExx or SxExx pattern in `s`. Returns (season, episode, rest_after).
fn find_sxexx(s: &str) -> Option<(u32, u32, &str)> {
    // Walk the string looking for 's' followed by digits, 'e', digits.
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].to_ascii_lowercase() == b's' {
            let start = i;
            i += 1;
            // Consume season digits (1-2 digits)
            let s_start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; }
            let s_end = i;
            if s_end == s_start || s_end - s_start > 2 { continue; }
            // Optional whitespace
            while i < bytes.len() && bytes[i] == b' ' { i += 1; }
            // Expect 'e'
            if i >= bytes.len() || bytes[i].to_ascii_lowercase() != b'e' {
                i = start + 1; continue;
            }
            i += 1;
            // Consume episode digits (2-3 digits)
            let e_start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; }
            let e_end = i;
            if e_end == e_start || e_end - e_start > 3 { i = start + 1; continue; }

            let season: u32 = s[s_start..s_end].parse().unwrap_or(1);
            let episode: u32 = s[e_start..e_end].parse().unwrap_or(0);
            // Make sure we're at a word boundary (not mid-word like "season")
            if start > 0 && bytes[start - 1].is_ascii_alphabetic() {
                i = start + 1; continue;
            }
            let after = s[i..].trim_start_matches(|c: char| c == ' ' || c == '-' || c == '–');
            return Some((season, episode, after));
        }
        i += 1;
    }
    None
}

/// Find a bare 2-3 digit episode number after a separator.
/// e.g. `Show - 08`, `Show_-_08_`
fn find_bare_episode_number(s: &str) -> Option<u32> {
    // Look for a dash/separator followed by 2-3 digits at end of meaningful content.
    let parts: Vec<&str> = s.split(|c| c == '-' || c == '–').collect();
    for part in parts.iter().rev() {
        let t = part.trim();
        if t.len() >= 2 && t.len() <= 3 && t.chars().all(|c| c.is_ascii_digit()) {
            if let Ok(n) = t.parse::<u32>() {
                if n > 0 && n < 1000 { return Some(n); }
            }
        }
    }
    None
}

/// Clean a candidate episode title string: strip leading/trailing separators,
/// collapse whitespace, and return empty string if nothing meaningful remains.
fn clean_episode_title(s: &str) -> String {
    // Strip leading dashes and whitespace
    let s = s.trim_matches(|c: char| c == '-' || c == '–' || c.is_whitespace());
    // If it starts with a bracket it's probably a release tag, not a title
    if s.starts_with('[') || s.starts_with('(') { return String::new(); }
    // Collapse internal whitespace
    let words: Vec<&str> = s.split_whitespace().collect();
    words.join(" ")
}
