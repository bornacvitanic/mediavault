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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct AppConfig {
    /// TMDB v3 API key. Leave empty to disable poster fetching.
    #[serde(default)]
    pub tmdb_api_key: String,
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

fn is_size_token(tok: &str) -> bool {
    let t = tok.trim_end_matches("mb").trim_end_matches("gb");
    !t.is_empty() && t.chars().all(|c| c.is_ascii_digit())
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
