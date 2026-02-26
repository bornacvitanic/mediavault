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
            DetailPanel::Movie(p) | DetailPanel::Show(p) => p.clone(),
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
            let detail_base = match &self.detail {
                DetailPanel::Movie(p) | DetailPanel::Show(p) => p.clone(),
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

                        match detail_base {
                            ref p if self.entries.iter().any(|e| {
                                matches!(e, MediaEntry::Movie(m) if m.base_dir == *p)
                            }) => {
                                render_movie_detail(ui, &detail_base, &mut self.entries, &mut self.comment_buf, &mut self.comment_dirty);
                            }
                            ref p if self.entries.iter().any(|e| {
                                matches!(e, MediaEntry::Show(s) if s.base_dir == *p)
                            }) => {
                                render_show_detail(ui, &detail_base, &mut self.entries, &mut self.comment_buf, &mut self.comment_dirty);
                            }
                            _ => {}
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
                let card_w = 160.0;
                let card_h = 260.0;
                let spacing = 12.0;
                let available_w = ui.available_width();
                let cols = ((available_w / (card_w + spacing)) as usize).max(1);

                // Pre-collect card data to avoid mid-loop borrow conflicts.
                let cards: Vec<(PathBuf, PathBuf, String, bool, bool, bool, bool)> = indices
                    .iter()
                    .map(|&idx| {
                        let entry = &self.entries[idx];
                        let base_dir = entry.base_dir().clone();
                        let poster_path = entry.poster_cache_path().clone();
                        let title = entry.title().to_string();
                        let is_movie = matches!(entry, MediaEntry::Movie(_));
                        let watched = is_watched(entry);
                        let in_progress = is_in_progress(entry);
                        let selected = match &self.detail {
                            DetailPanel::Movie(p) | DetailPanel::Show(p) => *p == base_dir,
                            DetailPanel::None => false,
                        };
                        (base_dir, poster_path, title, is_movie, watched, in_progress, selected)
                    })
                    .collect();

                for row_cards in cards.chunks(cols) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(spacing, 0.0);
                        for (base_dir, poster_path, title, is_movie, watched, in_progress, selected) in row_cards {
                            let texture = self.textures.get(poster_path).cloned();
                            let clicked = render_media_card(
                                ui,
                                title,
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
                                    DetailPanel::Movie(base_dir.clone())
                                } else {
                                    DetailPanel::Show(base_dir.clone())
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
// Allocates a fixed rect up front and draws everything manually via the painter
// so that layout direction never affects where poster vs title end up.
fn render_media_card(
    ui: &mut egui::Ui,
    title: &str,
    is_movie: bool,
    texture: Option<&TextureHandle>,
    watched: bool,
    in_progress: bool,
    selected: bool,
    card_w: f32,
    card_h: f32,
) -> bool {
    let inner_margin = 4.0;
    let content_w = card_w - inner_margin * 2.0;
    // Reserve space for two lines of title text below the poster.
    let poster_h = card_h - 40.0;

    // Allocate the full card rect first — this is what drives layout position
    // in the parent and gives us the click response without any overlay tricks.
    let (card_rect, card_response) = ui.allocate_exact_size(
        egui::vec2(card_w, card_h),
        egui::Sense::click(),
    );

    if ui.is_rect_visible(card_rect) {
        let border_color = if selected { egui::Color32::GOLD } else { egui::Color32::from_gray(60) };
        let border_width = if selected { 2.0 } else { 1.0 };

        let poster_rect = egui::Rect::from_min_size(
            card_rect.min + egui::vec2(inner_margin, inner_margin),
            egui::vec2(content_w, poster_h),
        );

        // Draw background and border first using a scoped painter borrow.
        {
            let painter = ui.painter();
            painter.rect_filled(card_rect, egui::Rounding::same(6.0), egui::Color32::from_gray(30));
            painter.rect_stroke(card_rect, egui::Rounding::same(6.0), egui::Stroke::new(border_width, border_color));
        }

        // Poster image or fallback icon — child_ui requires &mut ui, so painter must not be alive.
        if let Some(tex) = texture {
            let mut child = ui.child_ui(poster_rect, egui::Layout::top_down(egui::Align::Center));
            child.add(egui::Image::new(tex).fit_to_exact_size(poster_rect.size()));
        } else {
            let (bg, icon) = if is_movie {
                (egui::Color32::from_rgb(40, 40, 80), "MOVIE")
            } else {
                (egui::Color32::from_rgb(40, 60, 40), "SHOW")
            };
            ui.painter().rect_filled(poster_rect, 4.0, bg);
            ui.painter().text(
                poster_rect.center(),
                egui::Align2::CENTER_CENTER,
                icon,
                egui::FontId::proportional(48.0),
                egui::Color32::WHITE,
            );
        }

        // Watch status badge
        let badge = if watched {
            Some(("W", egui::Color32::from_rgb(80, 200, 80)))
        } else if in_progress {
            Some((">", egui::Color32::from_rgb(220, 200, 60)))
        } else {
            None
        };
        if let Some((glyph, color)) = badge {
            ui.painter().text(
                poster_rect.right_top() + egui::vec2(-4.0, 4.0),
                egui::Align2::RIGHT_TOP,
                glyph,
                egui::FontId::proportional(14.0),
                color,
            );
        }

        // Title below the poster — another child_ui, each call is a fresh short-lived borrow.
        let title_top = poster_rect.max.y + 4.0;
        let title_rect = egui::Rect::from_min_size(
            egui::pos2(card_rect.min.x + inner_margin, title_top),
            egui::vec2(content_w, card_rect.max.y - title_top - inner_margin),
        );
        let mut title_ui = ui.child_ui(title_rect, egui::Layout::top_down(egui::Align::LEFT));
        title_ui.add(
            egui::Label::new(
                egui::RichText::new(title)
                    .size(11.0)
                    .color(egui::Color32::WHITE),
            )
            .wrap(true),
        );

        if card_response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }

    card_response.clicked()
}


// ── Detail panels ─────────────────────────────────────────────────────────────

fn render_movie_detail(
    ui: &mut egui::Ui,
    base_dir: &Path,
    entries: &mut Vec<MediaEntry>,
    comment_buf: &mut String,
    comment_dirty: &mut bool,
) {
    let movie = match entries.iter_mut().find_map(|e| {
        if let MediaEntry::Movie(m) = e { if m.base_dir == base_dir { Some(m) } else { None } } else { None }
    }) {
        Some(m) => m,
        None => return,
    };

    ui.heading(&movie.title);
    ui.small(movie.video_path.to_string_lossy());
    ui.separator();

    // Watch status
    ui.label(if movie.state.watched { "Status: ✅ Watched" } else { "Status: ⬜ Unwatched" });
    if let Some(last) = movie.state.watch_history.last() {
        ui.small(format!("Last watched: {}", last.watched_at.format("%Y-%m-%d")));
    }
    ui.add_space(4.0);

    if ui.button("Open in Player").clicked() {
        open_in_player(&movie.video_path);
    }
    ui.add_space(4.0);

    if movie.state.watched {
        if ui.button("Log another watch").clicked() {
            movie.state.watch_history.push(WatchEvent { watched_at: Utc::now(), note: None });
            let _ = save_movie_state(&movie.base_dir, &movie.state);
        }
        if ui.button("Mark as Unwatched").clicked() {
            movie.state.watched = false;
            let _ = save_movie_state(&movie.base_dir, &movie.state);
        }
    } else {
        if ui.button("Mark as Watched").clicked() {
            movie.state.watched = true;
            movie.state.watch_history.push(WatchEvent { watched_at: Utc::now(), note: None });
            let _ = save_movie_state(&movie.base_dir, &movie.state);
        }
    }

    ui.separator();
    ui.label("Watch history:");
    if movie.state.watch_history.is_empty() {
        ui.small("— none yet —");
    } else {
        for event in &movie.state.watch_history {
            ui.small(format!("• {}", event.watched_at.format("%Y-%m-%d %H:%M")));
        }
    }

    ui.separator();
    render_comments_editor(ui, comment_buf, comment_dirty, base_dir);
}

fn render_show_detail(
    ui: &mut egui::Ui,
    base_dir: &Path,
    entries: &mut Vec<MediaEntry>,
    comment_buf: &mut String,
    comment_dirty: &mut bool,
) {
    let show = match entries.iter_mut().find_map(|e| {
        if let MediaEntry::Show(s) = e { if s.base_dir == base_dir { Some(s) } else { None } } else { None }
    }) {
        Some(s) => s,
        None => return,
    };

    ui.heading(&show.title);
    let watched = show.watched_count();
    let total = show.episode_count();
    ui.label(format!("Progress: {}/{} episodes watched", watched, total));
    ui.separator();

    // "Continue" button
    let next_up = show.bookmarks.next_up.clone().or_else(|| {
        // Default next_up: first unwatched episode
        show.all_episodes()
            .find(|ep| !show.bookmarks.is_watched(&ep.relative_path))
            .map(|ep| ep.relative_path.clone())
    });

    if let Some(ref rel) = next_up {
        let ep_title = show
            .all_episodes()
            .find(|ep| ep.relative_path == *rel)
            .map(|ep| ep.title.clone())
            .unwrap_or_else(|| rel.clone());

        if ui.button(format!("Continue: {}", ep_title)).clicked() {
            let video_path = show
                .all_episodes()
                .find(|ep| ep.relative_path == *rel)
                .map(|ep| ep.video_path.clone());
            if let Some(p) = video_path {
                open_in_player(&p);
            }
        }
    }

    ui.add_space(4.0);

    // Mark all / unmark all
    ui.horizontal(|ui| {
        if ui.button("Mark all watched").clicked() {
            let all: Vec<(String, Option<String>)> = {
                let eps: Vec<_> = show.all_episodes().collect();
                let rels: Vec<String> = eps.iter().map(|e| e.relative_path.clone()).collect();
                let mut pairs = Vec::new();
                for (i, r) in rels.iter().enumerate() {
                    let following = rels.get(i + 1).map(|s| s.as_str());
                    pairs.push((r.clone(), following.map(str::to_string)));
                }
                pairs
            };
            for (rel, _) in &all {
                if !show.bookmarks.is_watched(rel) {
                    show.bookmarks.watched_episodes.push(rel.clone());
                }
            }
            show.bookmarks.next_up = None;
            let _ = save_show_bookmarks(&show.base_dir, &show.bookmarks);
        }
        if ui.button("Clear all").clicked() {
            show.bookmarks.watched_episodes.clear();
            show.bookmarks.next_up = None;
            let _ = save_show_bookmarks(&show.base_dir, &show.bookmarks);
        }
    });

    ui.separator();

    // Episode list grouped by season
    let seasons: Vec<_> = show.seasons.iter().map(|s| {
        let label = s.label.clone();
        let eps: Vec<_> = s.episodes.iter().map(|ep| {
            (ep.title.clone(), ep.relative_path.clone(), ep.video_path.clone())
        }).collect();
        (label, eps)
    }).collect();
    let bookmarks = show.bookmarks.clone();
    let base = show.base_dir.clone();

    // flatten for "following episode" look-up
    let flat_rels: Vec<String> = show
        .all_episodes()
        .map(|ep| ep.relative_path.clone())
        .collect();

    for (season_label, episodes) in &seasons {
        ui.collapsing(season_label, |ui| {
            for (ep_title, rel, video_path) in episodes {
                let watched = bookmarks.is_watched(rel);
                let is_next = bookmarks.next_up.as_deref() == Some(rel.as_str());

                ui.horizontal(|ui| {
                    let check_label = if watched { "[W]" } else { "[ ]" };
                    if ui.small_button(check_label).clicked() {
                        // Toggle watch state — re-borrow mutably here.
                        // We must find the show again since we moved out of it above.
                        if let Some(s) = entries.iter_mut().find_map(|e| {
                            if let MediaEntry::Show(s) = e { if s.base_dir == base { Some(s) } else { None } } else { None }
                        }) {
                            if watched {
                                s.bookmarks.mark_unwatched(rel);
                            } else {
                                let idx = flat_rels.iter().position(|r| r == rel);
                                let following = idx.and_then(|i| flat_rels.get(i + 1)).map(String::as_str);
                                s.bookmarks.mark_watched(rel, following);
                            }
                            let _ = save_show_bookmarks(&s.base_dir, &s.bookmarks);
                        }
                    }

                    let mut label_text = egui::RichText::new(ep_title);
                    if is_next {
                        label_text = label_text.color(egui::Color32::YELLOW);
                    }
                    if watched {
                        label_text = label_text.color(egui::Color32::from_gray(120));
                    }

                    if ui.add(egui::Label::new(label_text).sense(egui::Sense::click())).clicked() {
                        open_in_player(video_path);
                    }
                });
            }
        });
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
