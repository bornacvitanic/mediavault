/// OpenSubtitles.com REST API integration for subtitle fetching.
///
/// Requires a free API key from https://www.opensubtitles.com/consumers
/// The key is stored in the shared `config.toml` alongside the TMDB key.
///
/// Blocking HTTP — call from a background thread in GUI/TUI contexts.
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde::Deserialize;

const API_BASE: &str = "https://api.opensubtitles.com/api/v1";

// ── OpenSubtitles file hash ──────────────────────────────────────────────────

/// Compute the OpenSubtitles hash for a video file.
///
/// Algorithm: sum of all 64-bit little-endian words in the first and last
/// 64 KiB of the file, plus the file size. Returned as a 16-char hex string.
pub fn compute_hash(path: &Path) -> Result<(String, u64), String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("open: {e}"))?;
    let file_size = file.metadata().map_err(|e| format!("metadata: {e}"))?.len();

    if file_size < 65536 {
        return Err("file too small for hashing".into());
    }

    let mut hash: u64 = file_size;
    let chunk_size: usize = 65536;
    let mut buf = vec![0u8; chunk_size];

    // First 64 KiB
    file.read_exact(&mut buf).map_err(|e| format!("read: {e}"))?;
    for chunk in buf.chunks_exact(8) {
        hash = hash.wrapping_add(u64::from_le_bytes(chunk.try_into().unwrap()));
    }

    // Last 64 KiB
    file.seek(SeekFrom::End(-(chunk_size as i64)))
        .map_err(|e| format!("seek: {e}"))?;
    file.read_exact(&mut buf).map_err(|e| format!("read: {e}"))?;
    for chunk in buf.chunks_exact(8) {
        hash = hash.wrapping_add(u64::from_le_bytes(chunk.try_into().unwrap()));
    }

    Ok((format!("{hash:016x}"), file_size))
}

// ── API response types ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SubtitleResult {
    pub file_id: u64,
    pub language: String,
    pub release: String,
    pub hearing_impaired: bool,
    pub download_count: u64,
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    data: Vec<SearchHit>,
}

#[derive(Deserialize)]
struct SearchHit {
    attributes: SearchAttributes,
}

#[derive(Deserialize)]
struct SearchAttributes {
    language: String,
    release: Option<String>,
    hearing_impaired: bool,
    download_count: u64,
    files: Vec<SubFile>,
}

#[derive(Deserialize)]
struct SubFile {
    file_id: u64,
}

#[derive(Deserialize)]
struct DownloadResponse {
    link: String,
    file_name: Option<String>,
}

// ── Search ───────────────────────────────────────────────────────────────────

/// Search for subtitles by file hash, falling back to title + year.
///
/// `languages` is a comma-separated list of ISO 639-1 codes (e.g. "en,de").
/// Pass an empty string to search all languages.
pub fn search_subtitles(
    api_key: &str,
    video_path: &Path,
    title: &str,
    year: Option<u32>,
    season: Option<u32>,
    episode: Option<u32>,
    languages: &str,
) -> Result<Vec<SubtitleResult>, String> {
    // Try hash-based search first (more accurate)
    if let Ok((hash, _)) = compute_hash(video_path) {
        let results = search_by_hash(api_key, &hash, languages)?;
        if !results.is_empty() {
            return Ok(results);
        }
    }

    // Fallback: title-based search
    search_by_title(api_key, title, year, season, episode, languages)
}

fn search_by_hash(
    api_key: &str,
    hash: &str,
    languages: &str,
) -> Result<Vec<SubtitleResult>, String> {
    let mut url = format!("{API_BASE}/subtitles?moviehash={hash}");
    if !languages.is_empty() {
        url.push_str(&format!("&languages={languages}"));
    }
    do_search(api_key, &url)
}

fn search_by_title(
    api_key: &str,
    title: &str,
    year: Option<u32>,
    season: Option<u32>,
    episode: Option<u32>,
    languages: &str,
) -> Result<Vec<SubtitleResult>, String> {
    let encoded = urlencoding::encode(title);
    let mut url = format!("{API_BASE}/subtitles?query={encoded}");
    if let Some(y) = year {
        url.push_str(&format!("&year={y}"));
    }
    if let Some(s) = season {
        url.push_str(&format!("&season_number={s}"));
    }
    if let Some(e) = episode {
        url.push_str(&format!("&episode_number={e}"));
    }
    if !languages.is_empty() {
        url.push_str(&format!("&languages={languages}"));
    }
    do_search(api_key, &url)
}

fn do_search(api_key: &str, url: &str) -> Result<Vec<SubtitleResult>, String> {
    let resp = ureq::get(url)
        .set("Api-Key", api_key)
        .set("User-Agent", "mediavault v0.1.4")
        .call()
        .map_err(|e| format!("API request failed: {e}"))?;

    let body: SearchResponse = resp
        .into_json()
        .map_err(|e| format!("failed to parse response: {e}"))?;

    let mut results: Vec<SubtitleResult> = body
        .data
        .into_iter()
        .filter_map(|hit| {
            let file_id = hit.attributes.files.first()?.file_id;
            Some(SubtitleResult {
                file_id,
                language: hit.attributes.language.clone(),
                release: hit
                    .attributes
                    .release
                    .unwrap_or_else(|| "Unknown".into()),
                hearing_impaired: hit.attributes.hearing_impaired,
                download_count: hit.attributes.download_count,
            })
        })
        .collect();

    // Sort by download count descending (most popular first)
    results.sort_by(|a, b| b.download_count.cmp(&a.download_count));
    Ok(results)
}

// ── Download ─────────────────────────────────────────────────────────────────

/// Download a subtitle file and save it next to the video.
///
/// Returns the path to the saved subtitle file.
pub fn download_subtitle(
    api_key: &str,
    file_id: u64,
    video_path: &Path,
    language: &str,
) -> Result<std::path::PathBuf, String> {
    // Request download link
    let resp = ureq::post(&format!("{API_BASE}/download"))
        .set("Api-Key", api_key)
        .set("User-Agent", "mediavault v0.1.4")
        .set("Content-Type", "application/json")
        .send_json(serde_json::json!({ "file_id": file_id }))
        .map_err(|e| format!("download request failed: {e}"))?;

    let dl: DownloadResponse = resp
        .into_json()
        .map_err(|e| format!("failed to parse download response: {e}"))?;

    // Fetch the actual subtitle file
    let sub_resp = ureq::get(&dl.link)
        .call()
        .map_err(|e| format!("failed to download subtitle file: {e}"))?;

    let mut sub_content = String::new();
    sub_resp
        .into_reader()
        .read_to_string(&mut sub_content)
        .map_err(|e| format!("failed to read subtitle content: {e}"))?;

    // Determine output filename
    let video_stem = video_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let ext = dl
        .file_name
        .as_deref()
        .and_then(|f| f.rsplit('.').next())
        .unwrap_or("srt");
    let out_name = format!("{video_stem}.{language}.{ext}");
    let out_path = video_path
        .parent()
        .unwrap_or(Path::new("."))
        .join(&out_name);

    std::fs::write(&out_path, &sub_content)
        .map_err(|e| format!("failed to write subtitle file: {e}"))?;

    Ok(out_path)
}
