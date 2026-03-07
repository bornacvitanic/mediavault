use chrono::Utc;
use media_core::models::WatchEvent;
use media_core::{save_movie_state, save_show_bookmarks, MediaEntry};
use std::time::Instant;

// ── Screens ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    Library,
    Detail, // shows movie or show detail depending on selected entry type
}

// ── Filters / sorts ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum WatchFilter {
    All,
    Watching,
    Unwatched,
    Watched,
}

impl WatchFilter {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Watching => "Watching",
            Self::Unwatched => "Unwatched",
            Self::Watched => "Watched",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::Watching,
            Self::Watching => Self::Unwatched,
            Self::Unwatched => Self::Watched,
            Self::Watched => Self::All,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum KindFilter {
    All,
    Movies,
    Shows,
}

impl KindFilter {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Movies => "Movies",
            Self::Shows => "Shows",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::Movies,
            Self::Movies => Self::Shows,
            Self::Shows => Self::All,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum SortBy {
    Title,
    RecentlyWatched,
    Progress,
    EpisodeCount,
    Year,
}

impl SortBy {
    pub fn label(self) -> &'static str {
        match self {
            Self::Title => "Title",
            Self::RecentlyWatched => "Recently Watched",
            Self::Progress => "Progress",
            Self::EpisodeCount => "Episode Count",
            Self::Year => "Year",
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::Title => Self::RecentlyWatched,
            Self::RecentlyWatched => Self::Progress,
            Self::Progress => Self::EpisodeCount,
            Self::EpisodeCount => Self::Year,
            Self::Year => Self::Title,
        }
    }
}

// ── Actions ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Action {
    Quit,
    Up,
    Down,
    PageUp,
    PageDown,
    Select,        // Enter
    Back,          // Esc / Backspace / left arrow (context-dependent)
    CycleFilter,   // f
    CycleKind,     // k
    CycleSort,     // s
    Play,          // p
    ToggleWatched, // space (episode list) or d (detail)
    MarkAllWatched,
    Notes,      // n
    SearchMode, // /
    Char(char),
    Backspace,
    Escape,
    Noop,
}

// ── Status message ────────────────────────────────────────────────────────────

pub struct StatusMsg {
    pub text: String,
    pub expires: Instant,
    pub is_error: bool,
}

impl StatusMsg {
    pub fn ok(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            expires: Instant::now() + std::time::Duration::from_secs(3),
            is_error: false,
        }
    }
    pub fn err(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            expires: Instant::now() + std::time::Duration::from_secs(4),
            is_error: true,
        }
    }
    pub fn expired(&self) -> bool {
        Instant::now() > self.expires
    }
}

// ── Main app state ────────────────────────────────────────────────────────────

pub struct App {
    pub entries: Vec<MediaEntry>,
    pub screen: Screen,

    // Library screen
    pub lib_selected: usize,
    pub lib_scroll: usize,
    pub watch_filter: WatchFilter,
    pub kind_filter: KindFilter,
    pub sort_by: SortBy,
    pub search: String,
    pub search_active: bool,

    // Detail screen
    pub detail_ep_selected: usize,
    pub detail_ep_scroll: usize,

    // Feedback
    pub status: Option<StatusMsg>,
}

impl App {
    pub fn new(entries: Vec<MediaEntry>) -> Self {
        Self {
            entries,
            screen: Screen::Library,
            lib_selected: 0,
            lib_scroll: 0,
            watch_filter: WatchFilter::All,
            kind_filter: KindFilter::All,
            sort_by: SortBy::Title,
            search: String::new(),
            search_active: false,
            detail_ep_selected: 0,
            detail_ep_scroll: 0,
            status: None,
        }
    }

    // ── Filtered + sorted index list ──────────────────────────────────────────

    pub fn visible_indices(&self) -> Vec<usize> {
        let q = self.search.to_lowercase();

        let mut indices: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                // Kind filter
                match self.kind_filter {
                    KindFilter::Movies => matches!(e, MediaEntry::Movie(_)),
                    KindFilter::Shows => matches!(e, MediaEntry::Show(_)),
                    KindFilter::All => true,
                }
            })
            .filter(|(_, e)| {
                // Watch filter
                match self.watch_filter {
                    WatchFilter::All => true,
                    WatchFilter::Watching => match e {
                        MediaEntry::Show(s) => {
                            let w = s.watched_count();
                            w > 0 && w < s.episode_count()
                        }
                        MediaEntry::Movie(_) => false,
                    },
                    WatchFilter::Unwatched => match e {
                        MediaEntry::Movie(m) => !m.state.watched,
                        MediaEntry::Show(s) => s.watched_count() == 0,
                    },
                    WatchFilter::Watched => match e {
                        MediaEntry::Movie(m) => m.state.watched,
                        MediaEntry::Show(s) => s.episode_count() > 0 && s.is_fully_watched(),
                    },
                }
            })
            .filter(|(_, e)| {
                // Search filter
                if q.is_empty() {
                    return true;
                }
                let meta = e.metadata();
                let title = if !meta.clean_title.is_empty() {
                    &meta.clean_title
                } else {
                    e.title()
                };
                title.to_lowercase().contains(&q)
            })
            .map(|(i, _)| i)
            .collect();

        // Sort
        indices.sort_by(|&a, &b| {
            let ea = &self.entries[a];
            let eb = &self.entries[b];
            match self.sort_by {
                SortBy::Title => {
                    let ta = display_title(ea).to_lowercase();
                    let tb = display_title(eb).to_lowercase();
                    ta.cmp(&tb)
                }
                SortBy::RecentlyWatched => last_watched(ea).cmp(&last_watched(eb)).reverse(),
                SortBy::Progress => progress_key(ea)
                    .partial_cmp(&progress_key(eb))
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .reverse(),
                SortBy::EpisodeCount => ep_count(ea).cmp(&ep_count(eb)).reverse(),
                SortBy::Year => {
                    let ya = ea.metadata().year.unwrap_or(0);
                    let yb = eb.metadata().year.unwrap_or(0);
                    yb.cmp(&ya)
                }
            }
        });

        indices
    }

    /// The entry currently selected in the library list.
    pub fn selected_entry(&self) -> Option<&MediaEntry> {
        let vis = self.visible_indices();
        vis.get(self.lib_selected).map(|&i| &self.entries[i])
    }

    /// Mutable index of the selected entry in self.entries.
    pub fn selected_entry_index(&self) -> Option<usize> {
        let vis = self.visible_indices();
        vis.get(self.lib_selected).copied()
    }

    pub fn tick(&mut self) {
        if let Some(msg) = &self.status {
            if msg.expired() {
                self.status = None;
            }
        }
    }

    pub fn set_status(&mut self, msg: StatusMsg) {
        self.status = Some(msg);
    }

    // ── Action handler ────────────────────────────────────────────────────────

    pub fn handle(&mut self, action: Action) {
        match self.screen {
            Screen::Library => self.handle_library(action),
            Screen::Detail => self.handle_detail(action),
        }
    }

    fn handle_library(&mut self, action: Action) {
        if self.search_active {
            match action {
                Action::Escape => {
                    self.search_active = false;
                    self.search.clear();
                    self.lib_selected = 0;
                }
                Action::Char(c) => {
                    self.search.push(c);
                    self.lib_selected = 0;
                }
                Action::Backspace => {
                    self.search.pop();
                    self.lib_selected = 0;
                }
                Action::Select => {
                    self.search_active = false;
                    self.open_detail();
                }
                Action::Up => self.lib_move(-1),
                Action::Down => self.lib_move(1),
                _ => {}
            }
            return;
        }

        match action {
            Action::Up => self.lib_move(-1),
            Action::Down => self.lib_move(1),
            Action::PageUp => self.lib_move(-10),
            Action::PageDown => self.lib_move(10),
            Action::Select => self.open_detail(),
            Action::CycleFilter => {
                self.watch_filter = self.watch_filter.next();
                self.lib_selected = 0;
                self.lib_scroll = 0;
            }
            Action::CycleKind => {
                self.kind_filter = self.kind_filter.next();
                self.lib_selected = 0;
                self.lib_scroll = 0;
            }
            Action::CycleSort => {
                self.sort_by = self.sort_by.next();
            }
            Action::SearchMode => {
                self.search_active = true;
                self.search.clear();
            }
            _ => {}
        }
    }

    fn open_detail(&mut self) {
        if self.selected_entry().is_none() {
            return;
        }
        self.detail_ep_selected = 0;
        self.detail_ep_scroll = 0;
        self.screen = Screen::Detail;
    }

    fn handle_detail(&mut self, action: Action) {
        match action {
            Action::Back => {
                self.screen = Screen::Library;
            }
            Action::Up => self.detail_move(-1),
            Action::Down => self.detail_move(1),
            Action::PageUp => self.detail_move(-10),
            Action::PageDown => self.detail_move(10),
            Action::Play => self.play_selected(),
            Action::Select => self.play_selected(),
            Action::ToggleWatched => self.toggle_watched(),
            Action::MarkAllWatched => self.mark_all_watched(),
            Action::Notes => self.open_notes(),
            _ => {}
        }
    }

    fn lib_move(&mut self, delta: i32) {
        let n = self.visible_indices().len();
        if n == 0 {
            return;
        }
        let new = (self.lib_selected as i32 + delta).clamp(0, n as i32 - 1) as usize;
        self.lib_selected = new;
    }

    fn detail_move(&mut self, delta: i32) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        let n = match entry {
            MediaEntry::Show(s) => s.episode_count(),
            MediaEntry::Movie(_) => 0,
        };
        if n == 0 {
            return;
        }
        let new = (self.detail_ep_selected as i32 + delta).clamp(0, n as i32 - 1) as usize;
        self.detail_ep_selected = new;
    }

    // ── Play ──────────────────────────────────────────────────────────────────

    pub fn play_selected(&mut self) {
        let Some(idx) = self.selected_entry_index() else {
            return;
        };
        match &self.entries[idx] {
            MediaEntry::Movie(m) => {
                let vp = m.video_path.clone();
                media_core::open_in_player(&vp);
                // Auto-mark watched
                if !m.state.watched {
                    if let MediaEntry::Movie(m2) = &mut self.entries[idx] {
                        m2.state.watched = true;
                        m2.state.watch_history.push(WatchEvent {
                            watched_at: Utc::now(),
                            note: None,
                        });
                        if let Err(e) = save_movie_state(&m2.video_path.clone(), &m2.state) {
                            self.set_status(StatusMsg::err(format!("save failed: {e}")));
                            return;
                        }
                    }
                    self.set_status(StatusMsg::ok("▶ Playing — marked as watched"));
                } else {
                    self.set_status(StatusMsg::ok("▶ Playing"));
                }
            }
            MediaEntry::Show(s) => {
                // Play focused episode (or next unwatched if on movie row)
                let ep_rel = {
                    let eps: Vec<&media_core::models::Episode> = s.all_episodes().collect();
                    if eps.is_empty() {
                        return;
                    }
                    let ep = eps
                        .get(self.detail_ep_selected)
                        .or_else(|| {
                            eps.iter()
                                .find(|ep| !s.bookmarks.is_watched(&ep.relative_path))
                        })
                        .copied();
                    ep.map(|e| e.relative_path.clone())
                };
                let Some(ep_rel) = ep_rel else {
                    return;
                };

                if let MediaEntry::Show(s2) = &self.entries[idx] {
                    let vp = s2
                        .all_episodes()
                        .find(|ep| ep.relative_path == ep_rel)
                        .map(|ep| ep.video_path.clone());
                    let Some(vp) = vp else {
                        return;
                    };
                    media_core::open_in_player(&vp);

                    let following = s2
                        .all_episodes()
                        .skip_while(|ep| ep.relative_path != ep_rel)
                        .nth(1)
                        .map(|ep| ep.relative_path.clone());
                    let base = s2.base_dir.clone();
                    let label = s2
                        .all_episodes()
                        .find(|ep| ep.relative_path == ep_rel)
                        .map(|ep| ep.display_label())
                        .unwrap_or_default();

                    if let MediaEntry::Show(s3) = &mut self.entries[idx] {
                        s3.bookmarks.mark_watched(&ep_rel, following.as_deref());
                        if let Err(e) = save_show_bookmarks(&base, &s3.bookmarks) {
                            self.set_status(StatusMsg::err(format!("save failed: {e}")));
                            return;
                        }
                    }
                    self.set_status(StatusMsg::ok(format!("▶ Playing {label}")));
                }
            }
        }
    }

    // ── Toggle watched ────────────────────────────────────────────────────────

    pub fn toggle_watched(&mut self) {
        let Some(idx) = self.selected_entry_index() else {
            return;
        };
        match &self.entries[idx] {
            MediaEntry::Movie(m) => {
                let new_state = !m.state.watched;
                let vp = m.video_path.clone();
                if let MediaEntry::Movie(m2) = &mut self.entries[idx] {
                    m2.state.watched = new_state;
                    if new_state {
                        m2.state.watch_history.push(WatchEvent {
                            watched_at: Utc::now(),
                            note: None,
                        });
                    }
                    let _ = save_movie_state(&vp, &m2.state);
                }
                let label = if new_state {
                    "✓ Marked as watched"
                } else {
                    "○ Marked as unwatched"
                };
                self.set_status(StatusMsg::ok(label));
            }
            MediaEntry::Show(s) => {
                // Toggle the focused episode
                let eps: Vec<String> = s.all_episodes().map(|e| e.relative_path.clone()).collect();
                let Some(ep_rel) = eps.get(self.detail_ep_selected).cloned() else {
                    return;
                };
                let is_watched = s.bookmarks.is_watched(&ep_rel);
                let base = s.base_dir.clone();

                if let MediaEntry::Show(s2) = &mut self.entries[idx] {
                    if is_watched {
                        s2.bookmarks.watched_episodes.retain(|p| p != &ep_rel);
                        s2.bookmarks.next_up = Some(ep_rel.clone());
                    } else {
                        let following = s2
                            .all_episodes()
                            .skip_while(|ep| ep.relative_path != ep_rel)
                            .nth(1)
                            .map(|ep| ep.relative_path.clone());
                        s2.bookmarks.mark_watched(&ep_rel, following.as_deref());
                    }
                    let _ = save_show_bookmarks(&base, &s2.bookmarks);
                }
                let label = if is_watched {
                    "○ Episode unmarked"
                } else {
                    "✓ Episode marked as watched"
                };
                self.set_status(StatusMsg::ok(label));
            }
        }
    }

    // ── Mark all ─────────────────────────────────────────────────────────────

    pub fn mark_all_watched(&mut self) {
        let Some(idx) = self.selected_entry_index() else {
            return;
        };
        if let MediaEntry::Show(s) = &self.entries[idx] {
            let base = s.base_dir.clone();
            let count = s.episode_count();
            if let MediaEntry::Show(s2) = &mut self.entries[idx] {
                for ep in s2.seasons.iter().flat_map(|s| s.episodes.iter()) {
                    s2.bookmarks.mark_watched(&ep.relative_path, None);
                }
                s2.bookmarks.next_up = None;
                let _ = save_show_bookmarks(&base, &s2.bookmarks);
            }
            self.set_status(StatusMsg::ok(format!(
                "✓ All {count} episodes marked as watched"
            )));
        }
    }

    // ── Notes ─────────────────────────────────────────────────────────────────

    pub fn open_notes(&mut self) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        let cp = entry.comments_path();

        // Ensure file exists
        if !cp.exists() {
            let _ = std::fs::write(&cp, "");
        }

        // Suspend TUI, open editor, resume
        let editor = std::env::var("EDITOR")
            .or_else(|_| std::env::var("VISUAL"))
            .unwrap_or_else(|_| {
                #[cfg(target_os = "windows")]
                {
                    "notepad".to_string()
                }
                #[cfg(not(target_os = "windows"))]
                {
                    "nano".to_string()
                }
            });

        // Restore terminal for editor
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen,);

        let _ = std::process::Command::new(&editor).arg(&cp).status();

        // Re-enter TUI
        let _ = crossterm::terminal::enable_raw_mode();
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::EnterAlternateScreen,);

        self.set_status(StatusMsg::ok("notes saved"));
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

pub fn display_title(entry: &MediaEntry) -> &str {
    let meta = entry.metadata();
    if !meta.clean_title.is_empty() {
        &meta.clean_title
    } else {
        entry.title()
    }
}

fn last_watched(entry: &MediaEntry) -> Option<chrono::DateTime<Utc>> {
    match entry {
        MediaEntry::Movie(m) => m.state.watch_history.iter().map(|e| e.watched_at).max(),
        MediaEntry::Show(s) => s
            .all_episodes()
            .filter(|ep| s.bookmarks.is_watched(&ep.relative_path))
            .filter_map(|ep| ep.video_mtime)
            .max(),
    }
}

fn progress_key(entry: &MediaEntry) -> f32 {
    match entry {
        MediaEntry::Movie(m) => {
            if m.state.watched {
                1.0
            } else {
                0.0
            }
        }
        MediaEntry::Show(s) => {
            let t = s.episode_count();
            if t == 0 {
                0.0
            } else {
                s.watched_count() as f32 / t as f32
            }
        }
    }
}

fn ep_count(entry: &MediaEntry) -> usize {
    match entry {
        MediaEntry::Show(s) => s.episode_count(),
        _ => 0,
    }
}
