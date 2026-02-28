use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use media_core::MediaEntry;
use crate::app::{App, display_title};

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let Some(entry) = app.selected_entry() else { return; };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),  // breadcrumb
            Constraint::Min(0),     // content
            Constraint::Length(3),  // footer
        ])
        .split(area);

    draw_breadcrumb(f, entry, chunks[0]);

    match entry {
        MediaEntry::Movie(_) => draw_movie(f, app, entry, chunks[1]),
        MediaEntry::Show(_)  => draw_show(f, app, entry, chunks[1]),
    }

    draw_footer(f, app, entry, chunks[2]);
}

// ── Breadcrumb ────────────────────────────────────────────────────────────────

fn draw_breadcrumb(f: &mut Frame, entry: &MediaEntry, area: Rect) {
    let title = display_title(entry);
    let kind = match entry { MediaEntry::Movie(_) => "Movie", MediaEntry::Show(_) => "Show" };
    let line = Line::from(vec![
        Span::styled(" Library", Style::default().fg(Color::DarkGray)),
        Span::styled(" › ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{title} "), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled(format!("[{kind}]"), Style::default().fg(Color::DarkGray)),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

// ── Movie detail ──────────────────────────────────────────────────────────────

fn draw_movie(f: &mut Frame, _app: &App, entry: &MediaEntry, area: Rect) {
    let MediaEntry::Movie(m) = entry else { return; };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),  // title + tags
            Constraint::Length(2),  // spacer + status
            Constraint::Min(0),     // watch history
        ])
        .split(area);

    // Title + metadata tags
    let title = if !m.metadata.clean_title.is_empty() { &m.metadata.clean_title } else { &m.title };
    let year = m.metadata.year.map(|y| format!(" ({y})")).unwrap_or_default();
    let tags = m.metadata.tags();

    let mut lines: Vec<Line> = vec![
        Line::from(vec![
            Span::raw(" "),
            Span::styled(
                format!("{title}{year}"),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    if !tags.is_empty() {
        let tag_spans: Vec<Span> = std::iter::once(Span::raw(" "))
            .chain(tags.iter().map(|t| {
                Span::styled(format!("[{t}]"), Style::default().fg(Color::Cyan))
            }).flat_map(|s| [s, Span::raw(" ")]))
            .collect();
        lines.push(Line::from(tag_spans));
    }

    lines.push(Line::raw(""));

    let status = if m.state.watched {
        Line::from(Span::styled(" ✓  Watched", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)))
    } else {
        Line::from(Span::styled(" ○  Unwatched", Style::default().fg(Color::DarkGray)))
    };
    lines.push(status);

    f.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        chunks[0],
    );

    // Watch history
    if !m.state.watch_history.is_empty() {
        let hist_lines: Vec<Line> = std::iter::once(
            Line::from(Span::styled(" Watch history", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)))
        ).chain(
            m.state.watch_history.iter().rev().take(8).map(|ev| {
                Line::from(Span::styled(
                    format!("   {}", ev.watched_at.format("%Y-%m-%d  %H:%M")),
                    Style::default().fg(Color::DarkGray),
                ))
            })
        ).collect();

        let block = Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(Color::DarkGray));
        f.render_widget(Paragraph::new(hist_lines).block(block), chunks[2]);
    }
}

// ── Show detail ───────────────────────────────────────────────────────────────

fn draw_show(f: &mut Frame, app: &App, entry: &MediaEntry, area: Rect) {
    let MediaEntry::Show(s) = entry else { return; };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),  // title + tags + progress bar
            Constraint::Min(0),     // episode list
        ])
        .split(area);

    // ── Header ────────────────────────────────────────────────────────────────
    let title = if !s.metadata.clean_title.is_empty() { &s.metadata.clean_title } else { &s.title };
    let watched = s.watched_count();
    let total = s.episode_count();
    let tags = s.metadata.tags();

    let bar = progress_bar_styled(watched, total, 24);
    let frac = format!("  {watched}/{total}");

    let mut header_lines: Vec<Line> = vec![
        Line::from(vec![
            Span::raw(" "),
            Span::styled(title.as_str(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
    ];

    if !tags.is_empty() {
        let tag_spans: Vec<Span> = std::iter::once(Span::raw(" "))
            .chain(tags.iter().map(|t| {
                Span::styled(format!("[{t}]"), Style::default().fg(Color::Cyan))
            }).flat_map(|s| [s, Span::raw(" ")]))
            .collect();
        header_lines.push(Line::from(tag_spans));
    } else {
        header_lines.push(Line::raw(""));
    }

    header_lines.push(Line::raw(""));

    let bar_style = if watched == total && total > 0 {
        Style::default().fg(Color::Green)
    } else if watched > 0 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    header_lines.push(Line::from(vec![
        Span::raw(" "),
        Span::styled(bar, bar_style),
        Span::styled(frac, Style::default().fg(Color::DarkGray)),
    ]));

    // Next up hint
    let next_rel = s.bookmarks.next_up.as_ref().cloned()
        .or_else(|| s.all_episodes().find(|ep| !s.bookmarks.is_watched(&ep.relative_path)).map(|ep| ep.relative_path.clone()));
    if let Some(ref nr) = next_rel {
        if let Some(ep) = s.all_episodes().find(|ep| ep.relative_path == *nr) {
            header_lines.push(Line::from(vec![
                Span::styled(" next: ", Style::default().fg(Color::DarkGray)),
                Span::styled(ep.display_label(), Style::default().fg(Color::Yellow)),
            ]));
        }
    } else {
        header_lines.push(Line::raw(""));
    }

    f.render_widget(Paragraph::new(header_lines), chunks[0]);

    // ── Episode list ──────────────────────────────────────────────────────────
    let all_eps: Vec<&media_core::models::Episode> = s.all_episodes().collect();
    let ep_count = all_eps.len();
    let visible_rows = chunks[1].height as usize;
    let scroll = compute_scroll(app.detail_ep_selected, visible_rows, ep_count);

    let mut current_season = u32::MAX;

    let items: Vec<ListItem> = all_eps[scroll..ep_count.min(scroll + visible_rows + 20)]
        .iter()
        .enumerate()
        .flat_map(|(i, ep)| {
            let abs_idx = scroll + i;
            let is_sel = abs_idx == app.detail_ep_selected;
            let is_watched = s.bookmarks.is_watched(&ep.relative_path);
            let is_next = next_rel.as_deref() == Some(ep.relative_path.as_str());

            let mut items: Vec<ListItem> = vec![];

            // Season header when season changes
            if ep.season_num != current_season && ep.season_num > 0 {
                current_season = ep.season_num;
                items.push(
                    ListItem::new(Line::from(Span::styled(
                        format!(" Season {}", ep.season_num),
                        Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
                    )))
                );
            }

            let (dot, dot_style) = if is_watched {
                ("✓", Style::default().fg(Color::Green))
            } else {
                ("○", Style::default().fg(Color::DarkGray))
            };

            let label = ep.display_label();
            let next_tag = if is_next {
                Span::styled("  ← next", Style::default().fg(Color::Yellow))
            } else {
                Span::raw("")
            };

            let label_style = if is_sel {
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
            } else if is_watched {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::White)
            };

            let line = Line::from(vec![
                Span::raw(if is_sel { " ▶ " } else { "   " }),
                Span::styled(dot, dot_style),
                Span::raw("  "),
                Span::styled(label, label_style),
                next_tag,
            ]);
            items.push(ListItem::new(line));
            items
        })
        .collect();

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::DarkGray));

    // Use a plain list (highlight via ▶ inline, not ratatui highlight_style,
    // because season headers shift indices)
    f.render_widget(List::new(items).block(block), chunks[1]);
}

// ── Footer ────────────────────────────────────────────────────────────────────

fn draw_footer(f: &mut Frame, app: &App, entry: &MediaEntry, area: Rect) {
    let content = if let Some(msg) = &app.status {
        let style = if msg.is_error { Style::default().fg(Color::Red) } else { Style::default().fg(Color::Green) };
        Line::from(Span::styled(format!(" {}", msg.text), style))
    } else {
        match entry {
            MediaEntry::Movie(_) => Line::from(vec![
                hint("←/esc", "back"),
                hint("enter/p", "play"),
                hint("d/space", "toggle watched"),
                hint("n", "notes"),
                hint("q", "quit"),
            ]),
            MediaEntry::Show(_) => Line::from(vec![
                hint("←/esc", "back"),
                hint("↑↓/jk", "episodes"),
                hint("enter/p", "play"),
                hint("space/d", "toggle watched"),
                hint("a", "mark all"),
                hint("n", "notes"),
                hint("q", "quit"),
            ]),
        }
    };

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::DarkGray));
    f.render_widget(Paragraph::new(content).block(block), area);
}

// ── Utilities ─────────────────────────────────────────────────────────────────

fn hint(key: &str, desc: &str) -> Span<'static> {
    Span::styled(
        format!("  [{key}] {desc}"),
        Style::default().fg(Color::DarkGray),
    )
}

fn progress_bar_styled(watched: usize, total: usize, width: usize) -> String {
    if total == 0 { return "○".repeat(width); }
    let filled = (watched * width) / total;
    (0..width).map(|i| if i < filled { '●' } else { '○' }).collect()
}

fn compute_scroll(selected: usize, visible: usize, total: usize) -> usize {
    if total <= visible { return 0; }
    if selected < visible / 2 { return 0; }
    (selected - visible / 2).min(total - visible)
}
