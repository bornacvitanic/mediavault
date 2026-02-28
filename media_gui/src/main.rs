// Tell Windows to use the GUI subsystem — hides the console window.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Command,
    thread,
};

use chrono::Utc;
use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use media_core::{
    models::{Comments, MediaEntry, Movie, Show, WatchEvent},
    sidecar::{
        load_comments, save_comments, save_movie_state, save_show_bookmarks,
    },
    tmdb::{fetch_poster, load_config, save_config, AppConfig},
    scan_library,
};

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MediaVault")
            .with_inner_size([1100.0, 720.0]),
        // Enable eframe persistence so the last library path survives restarts.
        persist_window: true,
        ..Default::default()
    };
    eframe::run_native("MediaVault", options, Box::new(|cc| Box::new(App::new(cc))))
}

// ── Background poster-fetch channel ──────────────────────────────────────────

/// Message sent from the background fetcher thread back to the UI thread once
/// a poster image has been loaded from disk (after being downloaded).
struct PosterLoaded {
    // Keyed on poster_path (not base_dir) so root-level movies sharing a
    // base_dir each get their own texture slot.
    poster_path: PathBuf,
    base_dir: PathBuf,
    image: ColorImage,
}

// ── Sort / filter state ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum FilterKind {
    All,
    Movies,
    Shows,
}

#[derive(Debug, Clone, PartialEq)]
enum SortBy {
    Title,
    DateAdded,
    WatchStatus,
}

#[derive(Debug, Clone, PartialEq)]
enum WatchFilter {
    All,
    Unwatched,
    Watched,
    InProgress,
}

// ── Detail panel ─────────────────────────────────────────────────────────────

enum DetailPanel {
    None,
    // Keyed on poster_path (unique per entry) rather than base_dir, because
    // root-level movies share a base_dir and would all match simultaneously.
    Movie(PathBuf),
    Show(PathBuf),
}

// ── Main application state ────────────────────────────────────────────────────

struct App {
    // Library
    library_root: Option<PathBuf>,
    entries: Vec<MediaEntry>,

    // Filtering / sorting
    filter_kind: FilterKind,
    watch_filter: WatchFilter,
    sort_by: SortBy,
    sort_asc: bool,
    search_query: String,

    // Detail panel
    detail: DetailPanel,
    /// Editing buffer for the comments panel.
    comment_buf: String,
    comment_dirty: bool,

    // Poster textures keyed by base_dir.
    textures: HashMap<PathBuf, TextureHandle>,
    poster_rx: Receiver<PosterLoaded>,
    poster_tx: Sender<PosterLoaded>,
    /// Tracks which entries have already had a poster fetch attempted so we
    /// don't re-spawn threads on every frame.
    poster_attempted: std::collections::HashSet<PathBuf>,

    // Card zoom (1.0 = default size)
    card_zoom: f32,

    // Config
    config: AppConfig,
    show_settings: bool,
    api_key_buf: String,
}

impl App {
    fn new(cc: &eframe::CreationContext) -> Self {
        let (poster_tx, poster_rx) = unbounded();
        let config = load_config();
        let api_key_buf = config.tmdb_api_key.clone();
        // Restore last library path from eframe persistent storage.
        let library_root = cc.storage
            .and_then(|s| eframe::get_value::<PathBuf>(s, "library_root"));
        let entries = library_root.as_deref()
            .map(media_core::scan_library)
            .unwrap_or_default();
        Self {
            library_root,
            entries,
            filter_kind: FilterKind::All,
            watch_filter: WatchFilter::All,
            sort_by: SortBy::Title,
            sort_asc: true,
            search_query: String::new(),
            detail: DetailPanel::None,
            comment_buf: String::new(),
            comment_dirty: false,
            textures: HashMap::new(),
            poster_rx,
            poster_tx,
            poster_attempted: Default::default(),
            card_zoom: 1.0,
            config,
            show_settings: false,
            api_key_buf,
        }
    }

    fn reload_library(&mut self) {
        if let Some(root) = &self.library_root.clone() {
            self.entries = scan_library(root);
            self.textures.clear();
            self.poster_attempted.clear();
            self.detail = DetailPanel::None;
        }
    }

    fn save_state(&self, storage: &mut dyn eframe::Storage) {
        if let Some(root) = &self.library_root {
            eframe::set_value(storage, "library_root", root);
        }
    }

    /// Kick off a background thread to fetch (or load from cache) a poster for
    /// the given entry. Does nothing if a fetch was already attempted.
    fn ensure_poster(&mut self, entry: &MediaEntry) {
        let base_dir = entry.base_dir().clone();
        let poster_path = entry.poster_cache_path().clone();
        if self.poster_attempted.contains(&poster_path) {
            return;
        }
        self.poster_attempted.insert(poster_path.clone());

        // If the poster is already cached on disk, just load it.
        if poster_path.exists() {
            let tx = self.poster_tx.clone();
            thread::spawn(move || {
                if let Ok(img) = load_image_from_disk(&poster_path) {
                    let _ = tx.send(PosterLoaded { poster_path, base_dir, image: img });
                }
            });
            return;
        }

        // Otherwise fetch from TMDB if we have an API key.
        if self.config.tmdb_api_key.is_empty() {
            return;
        }
        let api_key = self.config.tmdb_api_key.clone();
        let title = entry.title().to_string();
        let is_movie = matches!(entry, MediaEntry::Movie(_));
        let tx = self.poster_tx.clone();

        thread::spawn(move || {
            let ok = fetch_poster(&title, is_movie, &api_key, &poster_path)
                .unwrap_or(false);
            if ok {
                if let Ok(img) = load_image_from_disk(&poster_path) {
                    let _ = tx.send(PosterLoaded { poster_path: poster_path.clone(), base_dir, image: img });
                }
            }
        });
    }

    /// Drain the poster channel and upload any newly-arrived images as egui
    /// textures. Must be called each frame.
    fn poll_posters(&mut self, ctx: &egui::Context) {
        while let Ok(loaded) = self.poster_rx.try_recv() {
            let texture = ctx.load_texture(
                loaded.poster_path.to_string_lossy(),
                loaded.image,
                TextureOptions::LINEAR,
            );
            self.textures.insert(loaded.poster_path, texture);
        }
    }

    fn filtered_sorted_indices(&self) -> Vec<usize> {
        let q = self.search_query.to_lowercase();
        let mut indices: Vec<usize> = (0..self.entries.len())
            .filter(|&i| {
                let e = &self.entries[i];
                // Kind filter
                let kind_ok = match &self.filter_kind {
                    FilterKind::All => true,
                    FilterKind::Movies => matches!(e, MediaEntry::Movie(_)),
                    FilterKind::Shows => matches!(e, MediaEntry::Show(_)),
                };
                // Watch filter
                let watch_ok = match &self.watch_filter {
                    WatchFilter::All => true,
                    WatchFilter::Unwatched => !is_watched(e),
                    WatchFilter::Watched => is_watched(e),
                    WatchFilter::InProgress => is_in_progress(e),
                };
                // Search
                let search_ok = q.is_empty() || e.title().to_lowercase().contains(&q);
                kind_ok && watch_ok && search_ok
            })
            .collect();

        indices.sort_by(|&a, &b| {
            let ea = &self.entries[a];
            let eb = &self.entries[b];
            let cmp = match self.sort_by {
                SortBy::Title => ea.title().cmp(eb.title()),
                SortBy::DateAdded => {
                    ea.latest_video_mtime().cmp(&eb.latest_video_mtime())
                }
                SortBy::WatchStatus => {
                    watch_sort_key(ea).cmp(&watch_sort_key(eb))
                }
            };
            if self.sort_asc { cmp } else { cmp.reverse() }
        });

        indices
    }

    // ── Detail panel helpers ───────────────────────────────────────────────────

    fn open_detail(&mut self, entry: &MediaEntry) {
        // Save any pending comment before switching.
        self.flush_comments();
        let comments = load_comments(entry.base_dir());
        self.comment_buf = comments.markdown.clone();
        self.comment_dirty = false;
        self.detail = match entry {
            MediaEntry::Movie(m) => DetailPanel::Movie(m.base_dir.clone()),
            MediaEntry::Show(s) => DetailPanel::Show(s.base_dir.clone()),
        };
    }

    fn flush_comments(&mut self) {
        if !self.comment_dirty {
            return;
        }
        let base_dir = match &self.detail {
            DetailPanel::Movie(p) | DetailPanel::Show(p) => {
                // poster_path is the key; resolve back to base_dir for sidecar ops
                self.entries.iter()
                    .find(|e| e.poster_cache_path() == p)
                    .map(|e| e.base_dir().clone())
                    .unwrap_or_else(|| p.clone())
            }
            DetailPanel::None => return,
        };
        let comments = Comments { markdown: self.comment_buf.clone() };
        if let Err(e) = save_comments(&base_dir, &comments) {
            eprintln!("Failed to save comments: {}", e);
        }
        self.comment_dirty = false;
    }

    fn find_movie_mut(&mut self, base_dir: &Path) -> Option<&mut Movie> {
        self.entries.iter_mut().find_map(|e| {
            if let MediaEntry::Movie(m) = e {
                if m.base_dir == base_dir { Some(m) } else { None }
            } else {
                None
            }
        })
    }

    fn find_show_mut(&mut self, base_dir: &Path) -> Option<&mut Show> {
        self.entries.iter_mut().find_map(|e| {
            if let MediaEntry::Show(s) = e {
                if s.base_dir == base_dir { Some(s) } else { None }
            } else {
                None
            }
        })
    }
}

// ── eframe::App impl ──────────────────────────────────────────────────────────

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.save_state(storage);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_posters(ctx);

        // ── Top menu bar ──────────────────────────────────────────────────────
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("MediaVault");
                ui.separator();
                if ui.button("Open Library...").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.library_root = Some(path);
                        self.reload_library();
                    }
                }
                if self.library_root.is_some() && ui.button("Refresh").clicked() {
                    self.reload_library();
                }
                ui.separator();
                if ui.button("Settings").clicked() {
                    self.show_settings = !self.show_settings;
                }
            });
        });

        // ── Settings window ───────────────────────────────────────────────────
        if self.show_settings {
            let mut open = self.show_settings;
            egui::Window::new("Settings")
                .open(&mut open)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("TMDB API Key:");
                    ui.text_edit_singleline(&mut self.api_key_buf);
                    ui.small("Get a free key at https://www.themoviedb.org/settings/api");
                    ui.add_space(4.0);
                    if ui.button("Save").clicked() {
                        self.config.tmdb_api_key = self.api_key_buf.trim().to_string();
                        if let Err(e) = save_config(&self.config) {
                            eprintln!("Failed to save config: {}", e);
                        }
                    }
                });
            self.show_settings = open;
        }

        // ── Filter / sort bar ─────────────────────────────────────────────────
        if self.library_root.is_some() {
            egui::TopBottomPanel::top("filter_bar").show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Show:");
                    ui.selectable_value(&mut self.filter_kind, FilterKind::All, "All");
                    ui.selectable_value(&mut self.filter_kind, FilterKind::Movies, "Movies");
                    ui.selectable_value(&mut self.filter_kind, FilterKind::Shows, "Shows");
                    ui.separator();
                    ui.label("Watch:");
                    ui.selectable_value(&mut self.watch_filter, WatchFilter::All, "All");
                    ui.selectable_value(&mut self.watch_filter, WatchFilter::Unwatched, "Unwatched");
                    ui.selectable_value(&mut self.watch_filter, WatchFilter::Watched, "Watched");
                    ui.selectable_value(&mut self.watch_filter, WatchFilter::InProgress, "In Progress");
                    ui.separator();
                    ui.label("Zoom:");
                    ui.add(egui::Slider::new(&mut self.card_zoom, 0.5f32..=2.0).step_by(0.1).show_value(false));
                    ui.separator();
                    ui.label("Sort:");
                    ui.selectable_value(&mut self.sort_by, SortBy::Title, "Title");
                    ui.selectable_value(&mut self.sort_by, SortBy::DateAdded, "Date Added");
                    ui.selectable_value(&mut self.sort_by, SortBy::WatchStatus, "Watch Status");
                    let asc_label = if self.sort_asc { "Asc" } else { "Desc" };
                    if ui.button(asc_label).clicked() {
                        self.sort_asc = !self.sort_asc;
                    }
                    ui.separator();
                    ui.label("🔍");
                    ui.text_edit_singleline(&mut self.search_query);
                });
            });
        }

        // ── Detail panel (right side) ─────────────────────────────────────────
        let has_detail = !matches!(self.detail, DetailPanel::None);
        if has_detail {
            // DetailPanel is keyed on poster_path; resolve to base_dir for detail fns.
            let (detail_base, detail_is_movie) = match &self.detail {
                DetailPanel::Movie(poster) | DetailPanel::Show(poster) => {
                    let is_movie = matches!(self.detail, DetailPanel::Movie(_));
                    let base = self.entries.iter()
                        .find(|e| e.poster_cache_path() == poster)
                        .map(|e| e.base_dir().clone())
                        .unwrap_or_else(|| poster.clone());
                    (base, is_movie)
                }
                DetailPanel::None => unreachable!(),
            };

            egui::SidePanel::right("detail_panel")
                .min_width(340.0)
                .max_width(480.0)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        if ui.button("Close").clicked() {
                            self.flush_comments();
                            self.detail = DetailPanel::None;
                            return;
                        }
                        ui.separator();

                        if detail_is_movie {
                            render_movie_detail(ui, &detail_base, &mut self.entries, &self.textures, &mut self.comment_buf, &mut self.comment_dirty);
                        } else {
                            render_show_detail(ui, &detail_base, &mut self.entries, &self.textures, &mut self.comment_buf, &mut self.comment_dirty);
                        }
                    });
                });
        }

        // ── Main grid ────────────────────────────────────────────────────────
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.library_root.is_none() {
                ui.centered_and_justified(|ui| {
                    ui.label("Click 'Open Library…' to get started.");
                });
                return;
            }

            let indices = self.filtered_sorted_indices();
            if indices.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label("No media matched the current filters.");
                });
                return;
            }

            // Kick off poster fetches for all visible entries before drawing.
            // We need to clone keys to avoid borrowing issues.
            let base_dirs: Vec<PathBuf> = indices
                .iter()
                .map(|&i| self.entries[i].base_dir().clone())
                .collect();
            let is_movies: Vec<bool> = indices
                .iter()
                .map(|&i| matches!(self.entries[i], MediaEntry::Movie(_)))
                .collect();
            let titles: Vec<String> = indices
                .iter()
                .map(|&i| self.entries[i].title().to_string())
                .collect();
            let poster_paths: Vec<PathBuf> = indices
                .iter()
                .map(|&i| self.entries[i].poster_cache_path().clone())
                .collect();

            for (idx, bd) in base_dirs.iter().enumerate() {
                let is_movie = is_movies[idx];
                let title = &titles[idx];
                let poster_path = poster_paths[idx].clone();
                if !self.poster_attempted.contains(&poster_path) {
                    self.poster_attempted.insert(poster_path.clone());
                    let poster_path = poster_path;
                    let api_key = self.config.tmdb_api_key.clone();
                    let tx = self.poster_tx.clone();
                    let bd2 = bd.clone();
                    let title2 = title.clone();
                    let is_movie2 = is_movie;
                    thread::spawn(move || {
                        if poster_path.exists() {
                            if let Ok(img) = load_image_from_disk(&poster_path) {
                                let _ = tx.send(PosterLoaded { poster_path: poster_path.clone(), base_dir: bd2, image: img });
                            }
                            return;
                        }
                        if api_key.is_empty() { return; }
                        let ok = fetch_poster(&title2, is_movie2, &api_key, &poster_path)
                            .unwrap_or(false);
                        if ok {
                            if let Ok(img) = load_image_from_disk(&poster_path) {
                                let _ = tx.send(PosterLoaded { poster_path: poster_path.clone(), base_dir: bd2, image: img });
                            }
                        }
                    });
                }
            }

            egui::ScrollArea::vertical().show(ui, |ui| {
                let card_w = 150.0 * self.card_zoom;
                let card_h = 225.0 * self.card_zoom;  // 2:3 poster aspect ratio
                let spacing = 12.0;
                let available_w = ui.available_width();
                let cols = ((available_w / (card_w + spacing)) as usize).max(1);

                // Pre-collect card data to avoid mid-loop borrow conflicts.
                let cards: Vec<(PathBuf, PathBuf, String, Vec<String>, Option<(usize,usize)>, bool, bool, bool, bool)> = indices
                    .iter()
                    .map(|&idx| {
                        let entry = &self.entries[idx];
                        let base_dir = entry.base_dir().clone();
                        let poster_path = entry.poster_cache_path().clone();
                        let meta = entry.metadata();
                        let title = if meta.clean_title.is_empty() { entry.title().to_string() } else { meta.clean_title.clone() };
                        let tags = meta.tags();
                        let progress = match entry {
                            MediaEntry::Show(s) => Some((s.watched_count(), s.episode_count())),
                            MediaEntry::Movie(_) => None,
                        };
                        let is_movie = matches!(entry, MediaEntry::Movie(_));
                        let watched = is_watched(entry);
                        let in_progress = is_in_progress(entry);
                        let selected = match &self.detail {
                            DetailPanel::Movie(p) | DetailPanel::Show(p) => *p == poster_path,
                            DetailPanel::None => false,
                        };
                        (base_dir, poster_path, title, tags, progress, is_movie, watched, in_progress, selected)
                    })
                    .collect();

                for row_cards in cards.chunks(cols) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(spacing, 0.0);
                        for (base_dir, poster_path, title, tags, progress, is_movie, watched, in_progress, selected) in row_cards {
                            let texture = self.textures.get(poster_path).cloned();
                            let clicked = render_media_card(
                                ui,
                                title,
                                tags,
                                *progress,
                                *is_movie,
                                texture.as_ref(),
                                *watched,
                                *in_progress,
                                *selected,
                                card_w,
                                card_h,
                            );
                            if clicked {
                                self.flush_comments();
                                let comments = load_comments(base_dir);
                                self.comment_buf = comments.markdown;
                                self.comment_dirty = false;
                                self.detail = if *is_movie {
                                    DetailPanel::Movie(poster_path.clone())
                                } else {
                                    DetailPanel::Show(poster_path.clone())
                                };
                            }
                        }
                    });
                    ui.add_space(spacing);
                }
            });
        });
    }
}

// ── Card renderer ─────────────────────────────────────────────────────────────

// Returns true if the card was clicked.
//
// Visual states (via border color):
//   Unwatched    — neutral gray border
//   In progress  — amber border
//   Watched      — green border + dark dim overlay on poster
//   Selected     — gold border (overrides all)
//
// On hover:
//   - Border doubles in thickness
//   - Full-card semi-transparent overlay appears with:
//       - Clean title (center, large)
//       - Metadata tag pills (year, resolution, source, HDR) below title
//       - For shows: a slim progress bar at the very bottom
//
// When there is no poster the overlay is always shown (title always visible).
fn render_media_card(
    ui: &mut egui::Ui,
    title: &str,
    tags: &[String],
    show_progress: Option<(usize, usize)>,  // (watched, total) for shows
    is_movie: bool,
    texture: Option<&TextureHandle>,
    watched: bool,
    in_progress: bool,
    selected: bool,
    card_w: f32,
    card_h: f32,
) -> bool {
    let (card_rect, card_response) = ui.allocate_exact_size(
        egui::vec2(card_w, card_h),
        egui::Sense::click(),
    );

    if !ui.is_rect_visible(card_rect) {
        return card_response.clicked();
    }

    let hovered = card_response.hovered();

    let base_border_width: f32 = if selected { 2.5 } else { 1.0 };
    let border_width = if hovered { base_border_width * 2.0 } else { base_border_width };
    let border_color = if selected {
        egui::Color32::from_rgb(220, 180, 50)
    } else if watched {
        egui::Color32::from_rgb(55, 150, 55)
    } else if in_progress {
        egui::Color32::from_rgb(190, 130, 35)
    } else {
        egui::Color32::from_gray(50)
    };

    let rounding = egui::Rounding::same(6.0);
    let has_poster = texture.is_some();

    // Background
    ui.painter().rect_filled(card_rect, rounding, egui::Color32::from_gray(20));

    // Poster or fallback
    if let Some(tex) = texture {
        let mut child = ui.child_ui(card_rect, egui::Layout::top_down(egui::Align::Center));
        child.add(egui::Image::new(tex).fit_to_exact_size(card_rect.size()));
    } else {
        let (bg, label) = if is_movie {
            (egui::Color32::from_rgb(30, 30, 65), "MOVIE")
        } else {
            (egui::Color32::from_rgb(25, 50, 33), "SHOW")
        };
        ui.painter().rect_filled(card_rect, rounding, bg);
        ui.painter().text(
            card_rect.center() - egui::vec2(0.0, 10.0),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(16.0),
            egui::Color32::from_gray(130),
        );
    }

    // Watched dim overlay
    if watched {
        ui.painter().rect_filled(
            card_rect,
            rounding,
            egui::Color32::from_black_alpha(120),
        );
    }

    // Hover overlay — full card, or always-on when no poster
    let show_overlay = hovered || !has_poster;
    if show_overlay {
        // Semi-transparent full-card overlay
        if has_poster {
            ui.painter().rect_filled(
                card_rect,
                rounding,
                egui::Color32::from_black_alpha(165),
            );
        }

        let pad = card_w * 0.07;
        let center_x = card_rect.center().x;

        // Title — wrapped, clipped strictly to card bounds.
        // Uses a child_ui with painting disabled for interaction so hover
        // events pass through to the card response rect (no flicker).
        let title_font_size = (card_w * 0.095).clamp(10.0, 17.0);
        let title_area = egui::Rect::from_min_size(
            card_rect.min + egui::vec2(pad, card_h * 0.12),
            egui::vec2(card_w - pad * 2.0, card_h * 0.48),
        );
        // clip_rect ensures text never bleeds outside the card even when the
        // font is large and the title is long.
        // Draw title via painter (same approach as tag pills) — completely
        // avoids the egui widget style system which was dimming the text color.
        // Manual word-wrap: split into lines that fit within title_area width.
        {
            let max_w = title_area.width();
            // Approximate char width for the chosen font size
            let char_w = title_font_size * 0.55;
            let chars_per_line = ((max_w / char_w) as usize).max(1);
            let words: Vec<&str> = title.split_whitespace().collect();
            let mut lines: Vec<String> = Vec::new();
            let mut current = String::new();
            for word in &words {
                let candidate = if current.is_empty() {
                    word.to_string()
                } else {
                    format!("{current} {word}")
                };
                if candidate.len() <= chars_per_line {
                    current = candidate;
                } else {
                    if !current.is_empty() { lines.push(current.clone()); }
                    current = word.to_string();
                }
            }
            if !current.is_empty() { lines.push(current); }
            // Clamp to 3 lines max, truncate last with ellipsis if needed
            if lines.len() > 3 {
                lines.truncate(3);
                if let Some(last) = lines.last_mut() {
                    last.truncate(last.len().saturating_sub(1));
                    last.push('…');
                }
            }
            let line_h = title_font_size * 1.25;
            let block_h = lines.len() as f32 * line_h;
            let mut y = title_area.center().y - block_h / 2.0 + line_h / 2.0;
            for line in &lines {
                ui.painter().text(
                    egui::pos2(title_area.center().x, y),
                    egui::Align2::CENTER_CENTER,
                    line.as_str(),
                    egui::FontId::proportional(title_font_size),
                    egui::Color32::WHITE,
                );
                y += line_h;
            }
        }

        // Tag pills — flow layout: fill a row then start the next, up to 2 rows.
        // Tags are never truncated; they wrap instead.
        if !tags.is_empty() {
            let tag_font = egui::FontId::proportional((card_w * 0.072).clamp(8.0, 11.0));
            let tag_pill_h = tag_font.size + 6.0;
            let tag_pad_x = 6.0;
            let tag_gap = 4.0;
            let row_gap = 4.0;
            let max_row_w = card_w - pad * 2.0;
            let max_rows = 2usize;

            // Measure all tag widths upfront.
            let tag_widths: Vec<f32> = tags.iter()
                .map(|t| t.len() as f32 * tag_font.size * 0.6 + tag_pad_x * 2.0)
                .collect();

            // Split into rows, each row center-aligned.
            let mut rows: Vec<Vec<(usize, f32)>> = Vec::new();  // (tag_index, width)
            let mut current_row: Vec<(usize, f32)> = Vec::new();
            let mut current_w = 0.0f32;
            for (i, &tw) in tag_widths.iter().enumerate() {
                if rows.len() >= max_rows { break; }
                let needed = if current_row.is_empty() { tw } else { tw + tag_gap };
                if !current_row.is_empty() && current_w + needed > max_row_w {
                    rows.push(std::mem::take(&mut current_row));
                    current_w = 0.0;
                    if rows.len() >= max_rows { break; }
                }
                current_row.push((i, tw));
                current_w += if current_w == 0.0 { tw } else { tw + tag_gap };
            }
            if !current_row.is_empty() && rows.len() < max_rows {
                rows.push(current_row);
            }

            let total_tag_h = rows.len() as f32 * tag_pill_h + (rows.len() - 1).max(0) as f32 * row_gap;
            // Position tag block in the lower-middle of the overlay, above the progress bar.
            let tags_top = card_rect.min.y + card_h * 0.65 - total_tag_h / 2.0;

            let tag_border = egui::Color32::from_white_alpha(120);
            let tag_fg = egui::Color32::WHITE;

            for (row_idx, row) in rows.iter().enumerate() {
                let row_w: f32 = row.iter().map(|(_, tw)| tw).sum::<f32>()
                    + tag_gap * (row.len().saturating_sub(1)) as f32;
                let mut x = center_x - row_w / 2.0;
                let y = tags_top + row_idx as f32 * (tag_pill_h + row_gap);

                for (tag_idx, tw) in row {
                    let tag_rect = egui::Rect::from_min_size(
                        egui::pos2(x, y),
                        egui::vec2(*tw, tag_pill_h),
                    );
                    ui.painter().rect_stroke(tag_rect, egui::Rounding::same(3.0),
                        egui::Stroke::new(1.0, tag_border));
                    ui.painter().text(
                        tag_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        tags[*tag_idx].as_str(),
                        tag_font.clone(),
                        tag_fg,
                    );
                    x += tw + tag_gap;
                }
            }
        }

        // Show progress bar at the bottom of the overlay
        if let Some((watched_eps, total_eps)) = show_progress {
            if total_eps > 0 {
                let bar_h = (card_h * 0.035).clamp(3.0, 6.0);
                let bar_margin = pad;
                let bar_rect = egui::Rect::from_min_size(
                    egui::pos2(card_rect.min.x + bar_margin, card_rect.max.y - bar_h - 6.0),
                    egui::vec2(card_w - bar_margin * 2.0, bar_h),
                );
                let fill_w = bar_rect.width() * (watched_eps as f32 / total_eps as f32);

                // Track
                ui.painter().rect_filled(bar_rect, egui::Rounding::same(bar_h / 2.0),
                    egui::Color32::from_white_alpha(30));
                // Fill
                if fill_w > 0.0 {
                    let fill_rect = egui::Rect::from_min_size(bar_rect.min, egui::vec2(fill_w, bar_h));
                    let bar_color = if watched_eps == total_eps {
                        egui::Color32::from_rgb(55, 150, 55)
                    } else {
                        egui::Color32::from_rgb(190, 130, 35)
                    };
                    ui.painter().rect_filled(fill_rect, egui::Rounding::same(bar_h / 2.0), bar_color);
                }

                // Fraction label e.g. "6/10"
                let label = format!("{}/{}", watched_eps, total_eps);
                let label_font = egui::FontId::proportional((card_w * 0.07).clamp(8.0, 10.5));
                ui.painter().text(
                    egui::pos2(card_rect.max.x - bar_margin, bar_rect.min.y - 3.0),
                    egui::Align2::RIGHT_BOTTOM,
                    label,
                    label_font,
                    egui::Color32::from_gray(180),
                );
            }
        }
    }

    // Border always drawn last so it sits on top of everything
    ui.painter().rect_stroke(card_rect, rounding, egui::Stroke::new(border_width, border_color));

    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    card_response.clicked()
}

// ── Detail panels ─────────────────────────────────────────────────────────────

fn render_movie_detail(
    ui: &mut egui::Ui,
    base_dir: &Path,
    entries: &mut Vec<MediaEntry>,
    textures: &HashMap<PathBuf, TextureHandle>,
    comment_buf: &mut String,
    comment_dirty: &mut bool,
) {
    let movie = match entries.iter_mut().find_map(|e| {
        if let MediaEntry::Movie(m) = e { if m.base_dir == base_dir { Some(m) } else { None } } else { None }
    }) {
        Some(m) => m,
        None => return,
    };

    // ── Header: poster + title ────────────────────────────────────────────────
    let poster_key = movie.poster_path.clone();
    let clean = movie.metadata.clean_title.clone();
    let heading = if clean.is_empty() { movie.title.clone() } else { clean };
    let tags = movie.metadata.tags();
    let video_path = movie.video_path.clone();
    let poster_tex = textures.get(&poster_key).cloned();

    ui.horizontal(|ui| {
        if let Some(tex) = &poster_tex {
            ui.add(egui::Image::new(tex).max_size(egui::vec2(80.0, 120.0)).rounding(4.0));
        }
        ui.vertical(|ui| {
            ui.heading(&heading);
            ui.label(egui::RichText::new(
                video_path.file_name().unwrap_or_default().to_string_lossy())
                .size(10.0).color(egui::Color32::from_gray(130)));
            if !tags.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    for tag in &tags {
                        ui.label(egui::RichText::new(tag).size(10.0)
                            .color(egui::Color32::from_gray(160)));
                    }
                });
            }
        });
    });

    ui.separator();

    // ── Actions ───────────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        if ui.button("Open in Player").clicked() {
            open_in_player(&movie.video_path);
        }
        if movie.state.watched {
            if ui.button("Mark Unwatched").clicked() {
                movie.state.watched = false;
                let _ = save_movie_state(&movie.base_dir, &movie.state);
            }
            if ui.button("Log rewatch").clicked() {
                movie.state.watch_history.push(WatchEvent { watched_at: Utc::now(), note: None });
                let _ = save_movie_state(&movie.base_dir, &movie.state);
            }
        } else {
            if ui.button("Mark Watched").clicked() {
                movie.state.watched = true;
                movie.state.watch_history.push(WatchEvent { watched_at: Utc::now(), note: None });
                let _ = save_movie_state(&movie.base_dir, &movie.state);
            }
        }
    });

    if !movie.state.watch_history.is_empty() {
        ui.add_space(2.0);
        for event in movie.state.watch_history.iter().rev().take(5) {
            ui.label(egui::RichText::new(format!("  Watched {}", event.watched_at.format("%Y-%m-%d")))
                .size(11.0).color(egui::Color32::from_gray(140)));
        }
    }

    ui.separator();
    render_comments_editor(ui, comment_buf, comment_dirty, base_dir);
}

fn render_show_detail(
    ui: &mut egui::Ui,
    base_dir: &Path,
    entries: &mut Vec<MediaEntry>,
    textures: &HashMap<PathBuf, TextureHandle>,
    comment_buf: &mut String,
    comment_dirty: &mut bool,
) {
    // ── Collect display data (immutable borrow) ───────────────────────────────
    let (poster_key, heading, tags, watched_count, total, next_path, seasons_data) = {
        let show = match entries.iter().find_map(|e| {
            if let MediaEntry::Show(s) = e { if s.base_dir == base_dir { Some(s) } else { None } } else { None }
        }) {
            Some(s) => s,
            None => return,
        };
        let clean = show.metadata.clean_title.clone();
        let heading = if clean.is_empty() { show.title.clone() } else { clean };
        let next = show.bookmarks.next_up.clone().or_else(|| {
            show.all_episodes()
                .find(|ep| !show.bookmarks.is_watched(&ep.relative_path))
                .map(|ep| ep.relative_path.clone())
        });
        // Collect season/episode data for rendering
        let seasons_data: Vec<(String, Vec<(String, String, Option<String>, bool, bool, PathBuf)>)> =
            show.seasons.iter().map(|s| {
                let eps = s.episodes.iter().map(|ep| (
                    ep.relative_path.clone(),
                    ep.display_label(),
                    ep.episode_title.clone(),
                    show.bookmarks.is_watched(&ep.relative_path),
                    show.bookmarks.next_up.as_deref() == Some(&ep.relative_path),
                    ep.video_path.clone(),
                )).collect();
                (s.label.clone(), eps)
            }).collect();
        (
            show.poster_path.clone(),
            heading,
            show.metadata.tags(),
            show.watched_count(),
            show.episode_count(),
            next,
            seasons_data,
        )
    };

    let poster_tex = textures.get(&poster_key).cloned();

    // ── Header ────────────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        if let Some(tex) = &poster_tex {
            ui.add(egui::Image::new(tex).max_size(egui::vec2(80.0, 120.0)).rounding(4.0));
        }
        ui.vertical(|ui| {
            ui.heading(&heading);
            if !tags.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    for tag in &tags {
                        ui.label(egui::RichText::new(tag).size(10.0)
                            .color(egui::Color32::from_gray(160)));
                    }
                });
            }
            ui.add_space(4.0);
            let progress = if total > 0 { watched_count as f32 / total as f32 } else { 0.0 };
            ui.add(egui::ProgressBar::new(progress)
                .text(format!("{}/{} episodes", watched_count, total))
                .desired_width(160.0));
        });
    });

    ui.separator();

    // ── Bulk actions + Continue ───────────────────────────────────────────────
    ui.horizontal(|ui| {
        // Continue button — shows next episode label
        if let Some(ref np) = next_path {
            let next_label = seasons_data.iter()
                .flat_map(|(_, eps)| eps.iter())
                .find(|(rp, _, _, _, _, _)| rp == np)
                .map(|(_, label, _, _, _, _)| label.clone())
                .unwrap_or_else(|| "Next".into());
            if ui.button(format!("Continue  {next_label}")).clicked() {
                if let Some((_, _, _, _, _, vp)) = seasons_data.iter()
                    .flat_map(|(_, eps)| eps.iter())
                    .find(|(rp, _, _, _, _, _)| rp == np)
                {
                    open_in_player(vp);
                }
            }
        }
        if ui.button("Mark all").clicked() {
            if let Some(show) = entries.iter_mut().find_map(|e| {
                if let MediaEntry::Show(s) = e { if s.base_dir == base_dir { Some(s) } else { None } } else { None }
            }) {
                let all: Vec<String> = show.all_episodes().map(|ep| ep.relative_path.clone()).collect();
                for p in &all { show.bookmarks.mark_watched(p, None); }
                show.bookmarks.next_up = None;
                let _ = save_show_bookmarks(&show.base_dir, &show.bookmarks);
            }
        }
        if ui.button("Clear all").clicked() {
            if let Some(show) = entries.iter_mut().find_map(|e| {
                if let MediaEntry::Show(s) = e { if s.base_dir == base_dir { Some(s) } else { None } } else { None }
            }) {
                show.bookmarks.watched_episodes.clear();
                show.bookmarks.next_up = None;
                let _ = save_show_bookmarks(&show.base_dir, &show.bookmarks);
            }
        }
    });

    ui.separator();

    // ── Episode list ──────────────────────────────────────────────────────────
    // Collect mutations during rendering, apply after.
    let mut toggle_path: Option<String> = None;
    let mut open_path: Option<PathBuf> = None;
    let multi_season = seasons_data.len() > 1;

    egui::ScrollArea::vertical().id_source("ep_scroll").show(ui, |ui| {
        for (season_label, episodes) in &seasons_data {
            if multi_season {
                ui.add_space(3.0);
                ui.label(egui::RichText::new(season_label)
                    .size(11.0).color(egui::Color32::from_gray(140)).strong());
                ui.add_space(1.0);
            }
            for (rel_path, label, _ep_title, is_watched, is_next, video_path) in episodes {
                ui.horizontal(|ui| {
                    // Watched dot indicator instead of checkbox to avoid borrow issue
                    let dot_color = if *is_watched {
                        egui::Color32::from_rgb(55, 150, 55)
                    } else {
                        egui::Color32::from_gray(60)
                    };
                    let (dot_rect, dot_resp) = ui.allocate_exact_size(
                        egui::vec2(10.0, 10.0), egui::Sense::click()
                    );
                    ui.painter().circle_filled(dot_rect.center(), 4.0, dot_color);
                    if dot_resp.clicked() {
                        toggle_path = Some(rel_path.clone());
                    }
                    if dot_resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        ui.painter().circle_stroke(dot_rect.center(), 4.0,
                            egui::Stroke::new(1.0, egui::Color32::WHITE));
                    }

                    // Episode label
                    let text_color = if *is_next {
                        egui::Color32::from_rgb(210, 160, 50)
                    } else if *is_watched {
                        egui::Color32::from_gray(100)
                    } else {
                        egui::Color32::from_gray(210)
                    };
                    let resp = ui.add(
                        egui::Label::new(egui::RichText::new(label).size(12.0).color(text_color))
                            .sense(egui::Sense::click())
                    );
                    if resp.clicked() {
                        open_path = Some(video_path.clone());
                    }
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }

                    if *is_next {
                        ui.label(egui::RichText::new("next").size(9.0)
                            .color(egui::Color32::from_rgb(190, 140, 40)));
                    }
                });
            }
        }
    });

    // Apply mutations
    if let Some(ref path) = toggle_path {
        if let Some(show) = entries.iter_mut().find_map(|e| {
            if let MediaEntry::Show(s) = e { if s.base_dir == base_dir { Some(s) } else { None } } else { None }
        }) {
            if show.bookmarks.is_watched(path) {
                show.bookmarks.mark_unwatched(path);
            } else {
                // Find the episode that follows this one to auto-advance next_up
                let following = show.all_episodes()
                    .skip_while(|ep| &ep.relative_path != path)
                    .nth(1)
                    .map(|ep| ep.relative_path.clone());
                show.bookmarks.mark_watched(path, following.as_deref());
            }
            let _ = save_show_bookmarks(&show.base_dir, &show.bookmarks);
        }
    }
    if let Some(ref vp) = open_path {
        open_in_player(vp);
    }

    ui.separator();
    render_comments_editor(ui, comment_buf, comment_dirty, base_dir);
}

fn render_comments_editor(
    ui: &mut egui::Ui,
    comment_buf: &mut String,
    comment_dirty: &mut bool,
    base_dir: &Path,
) {
    ui.label("Notes (markdown):");
    let resp = ui.add(
        egui::TextEdit::multiline(comment_buf)
            .desired_rows(6)
            .desired_width(f32::INFINITY),
    );
    if resp.changed() {
        *comment_dirty = true;
    }
    if *comment_dirty && ui.button("Save notes").clicked() {
        let comments = Comments { markdown: comment_buf.clone() };
        if let Err(e) = save_comments(base_dir, &comments) {
            eprintln!("Failed to save comments: {}", e);
        }
        *comment_dirty = false;
    }
}

// ── Utilities ─────────────────────────────────────────────────────────────────

fn is_watched(entry: &MediaEntry) -> bool {
    match entry {
        MediaEntry::Movie(m) => m.state.watched,
        MediaEntry::Show(s) => s.is_fully_watched(),
    }
}

fn is_in_progress(entry: &MediaEntry) -> bool {
    match entry {
        MediaEntry::Movie(_) => false,
        MediaEntry::Show(s) => {
            let wc = s.watched_count();
            wc > 0 && wc < s.episode_count()
        }
    }
}

fn watch_sort_key(entry: &MediaEntry) -> u8 {
    if is_watched(entry) { 2 } else if is_in_progress(entry) { 1 } else { 0 }
}

fn open_in_player(path: &Path) {
    // Uses the Windows default file association, which is typically VLC or
    // Windows Media Player depending on the user's setup.
    #[cfg(target_os = "windows")]
    let _ = Command::new("cmd")
        .args(["/c", "start", "", &path.to_string_lossy()])
        .spawn();

    #[cfg(not(target_os = "windows"))]
    let _ = Command::new("xdg-open").arg(path).spawn();
}

fn load_image_from_disk(path: &Path) -> Result<ColorImage, Box<dyn std::error::Error>> {
    let data = std::fs::read(path)?;
    let img = image::load_from_memory(&data)?.to_rgba8();
    let (w, h) = img.dimensions();
    let pixels: Vec<egui::Color32> = img
        .pixels()
        .map(|p| egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
        .collect();
    Ok(ColorImage { size: [w as usize, h as usize], pixels })
}
