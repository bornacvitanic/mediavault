use media_core::{MediaEntry, models::Episode};

// ── Colours (ANSI, disabled when not a tty) ───────────────────────────────────

pub fn is_tty() -> bool {
    // Simple check: if stdout is piped, skip colour
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        unsafe { libc_isatty(std::io::stdout().as_raw_fd()) }
    }
    #[cfg(not(unix))]
    { true }
}

#[cfg(unix)]
extern "C" { fn isatty(fd: i32) -> i32; }
#[cfg(unix)]
fn libc_isatty(fd: i32) -> bool { unsafe { isatty(fd) != 0 } }

pub struct Style {
    pub tty: bool,
}

impl Style {
    pub fn new() -> Self { Self { tty: is_tty() } }

    pub fn green<'a>(&self, s: &'a str) -> String {
        if self.tty { format!("\x1b[32m{s}\x1b[0m") } else { s.to_string() }
    }
    pub fn yellow<'a>(&self, s: &'a str) -> String {
        if self.tty { format!("\x1b[33m{s}\x1b[0m") } else { s.to_string() }
    }
    pub fn dim<'a>(&self, s: &'a str) -> String {
        if self.tty { format!("\x1b[2m{s}\x1b[0m") } else { s.to_string() }
    }
    pub fn bold<'a>(&self, s: &'a str) -> String {
        if self.tty { format!("\x1b[1m{s}\x1b[0m") } else { s.to_string() }
    }
    pub fn cyan<'a>(&self, s: &'a str) -> String {
        if self.tty { format!("\x1b[36m{s}\x1b[0m") } else { s.to_string() }
    }
}

// ── Progress bar ─────────────────────────────────────────────────────────────

/// Render a compact text progress bar: ●●●●○○○○  6/10
pub fn progress_bar(watched: usize, total: usize, width: usize) -> String {
    if total == 0 { return String::new(); }
    let filled = (watched * width) / total;
    let bar: String = (0..width)
        .map(|i| if i < filled { '●' } else { '○' })
        .collect();
    format!("{}  {}/{}", bar, watched, total)
}

// ── Entry display helpers ─────────────────────────────────────────────────────

pub fn entry_display_title(entry: &MediaEntry) -> &str {
    let meta = entry.metadata();
    if !meta.clean_title.is_empty() { &meta.clean_title } else { entry.title() }
}

/// Single-line entry summary for list views.
pub fn entry_summary_line(entry: &MediaEntry, st: &Style) -> String {
    match entry {
        media_core::MediaEntry::Movie(m) => {
            let title = if !m.metadata.clean_title.is_empty() { &m.metadata.clean_title } else { &m.title };
            let year = m.metadata.year.map(|y| format!(" ({})", y)).unwrap_or_default();
            let status = if m.state.watched {
                st.green("✓ Watched")
            } else {
                st.dim("○ Unwatched")
            };
            format!("{}{:<2}  {}", st.bold(title), year, status)
        }
        media_core::MediaEntry::Show(s) => {
            let title = if !s.metadata.clean_title.is_empty() { &s.metadata.clean_title } else { &s.title };
            let watched = s.watched_count();
            let total = s.episode_count();
            let bar = progress_bar(watched, total, 10);
            let season_tag = s.metadata.season
                .as_ref()
                .map(|(n, _)| format!(" S{:02}", n))
                .unwrap_or_default();
            if watched == total && total > 0 {
                format!("{}{:<2}  {}", st.bold(title), season_tag, st.green(&bar))
            } else if watched > 0 {
                format!("{}{:<2}  {}", st.bold(title), season_tag, st.yellow(&bar))
            } else {
                format!("{}{:<2}  {}", st.bold(title), season_tag, st.dim(&bar))
            }
        }
    }
}

/// Format an episode for list display.
pub fn episode_line(ep: &Episode, is_watched: bool, is_next: bool, st: &Style) -> String {
    let dot = if is_watched { st.green("✓") } else { st.dim("○") };
    let label = ep.display_label();
    let next_tag = if is_next { st.cyan("  ← next") } else { String::new() };
    if is_watched {
        format!("  {}  {}{}", dot, st.dim(&label), next_tag)
    } else {
        format!("  {}  {}{}", dot, label, next_tag)
    }
}
