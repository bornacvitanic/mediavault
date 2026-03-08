use crate::app::{display_title, App};
use mediavault_core::MediaEntry;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let vis = app.visible_indices();
    let n = vis.len();

    // ── Layout: header / list / footer ───────────────────────────────────────
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header + filter bar
            Constraint::Min(0),    // list
            Constraint::Length(3), // footer / search / status
        ])
        .split(area);

    draw_header(f, app, chunks[0]);
    draw_list(f, app, &vis, chunks[1]);
    draw_footer(f, app, n, chunks[2]);
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let total = app.entries.len();
    let vis_n = app.visible_indices().len();

    let title = Span::styled(
        " MediaVault ",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );
    let counts = Span::styled(
        format!(" {vis_n}/{total} entries "),
        Style::default().fg(Color::DarkGray),
    );

    // Filter / sort pills
    let filter_s = Span::styled(
        format!(" {} ", app.watch_filter.label()),
        Style::default().fg(Color::Yellow),
    );
    let kind_s = Span::styled(
        format!(" {} ", app.kind_filter.label()),
        Style::default().fg(Color::Yellow),
    );
    let sort_s = Span::styled(
        format!(" {} ", app.sort_by.label()),
        Style::default().fg(Color::Magenta),
    );

    let header_line = Line::from(vec![
        title,
        counts,
        Span::raw("  "),
        Span::styled("Filter:", Style::default().fg(Color::DarkGray)),
        filter_s,
        Span::styled("Type:", Style::default().fg(Color::DarkGray)),
        kind_s,
        Span::styled("Sort:", Style::default().fg(Color::DarkGray)),
        sort_s,
    ]);

    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(Color::DarkGray));

    let para = Paragraph::new(header_line).block(block);
    f.render_widget(para, area);
}

fn draw_list(f: &mut Frame, app: &App, vis: &[usize], area: Rect) {
    let items: Vec<ListItem> = vis
        .iter()
        .enumerate()
        .map(|(pos, &idx)| {
            let entry = &app.entries[idx];
            let is_selected = pos == app.lib_selected;
            make_list_item(entry, is_selected, area.width)
        })
        .collect();

    let list = List::new(items).block(Block::default()).highlight_style(
        Style::default()
            .bg(Color::Rgb(40, 40, 60))
            .add_modifier(Modifier::BOLD),
    );

    // Compute scroll so selected item is always visible
    let visible_rows = area.height as usize;
    let scroll = compute_scroll(app.lib_selected, visible_rows, vis.len());

    let mut state = ListState::default();
    state.select(Some(app.lib_selected));
    // Offset — ratatui ListState doesn't expose offset directly so we
    // slice the items and adjust selected index.
    let end = (scroll + visible_rows).min(vis.len());
    let sliced: Vec<ListItem> =
        items_slice(vis, &app.entries, scroll, end, app.lib_selected, area.width);
    let adj_selected = app.lib_selected.saturating_sub(scroll);

    let list2 = List::new(sliced).block(Block::default()).highlight_style(
        Style::default()
            .bg(Color::Rgb(40, 40, 60))
            .add_modifier(Modifier::BOLD),
    );
    let mut state2 = ListState::default();
    state2.select(Some(adj_selected));
    f.render_stateful_widget(list2, area, &mut state2);
    let _ = list; // suppress unused warning
    let _ = state;
}

fn items_slice<'a>(
    vis: &'_ [usize],
    entries: &'a [MediaEntry],
    start: usize,
    end: usize,
    selected: usize,
    width: u16,
) -> Vec<ListItem<'a>> {
    vis[start..end]
        .iter()
        .enumerate()
        .map(|(i, &idx)| {
            let entry = &entries[idx];
            make_list_item(entry, start + i == selected, width)
        })
        .collect()
}

fn make_list_item(entry: &MediaEntry, _selected: bool, width: u16) -> ListItem<'_> {
    let title = display_title(entry);
    let max_title = (width as usize).saturating_sub(32).clamp(16, 40);
    let truncated = truncate(title, max_title);

    match entry {
        MediaEntry::Movie(m) => {
            let (status_sym, status_style) = if m.state.watched {
                ("✓", Style::default().fg(Color::Green))
            } else {
                ("○", Style::default().fg(Color::DarkGray))
            };
            let year = m
                .metadata
                .year
                .map(|y| format!(" ({y})"))
                .unwrap_or_default();
            let tags = m.metadata.tags();
            let tag_str = if tags.is_empty() {
                String::new()
            } else {
                format!("  {}", tags.join(" · "))
            };

            Line::from(vec![
                Span::styled(format!(" {status_sym} "), status_style),
                Span::styled(
                    format!("{truncated:<max_title$}"),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(year, Style::default().fg(Color::DarkGray)),
                Span::styled(tag_str, Style::default().fg(Color::DarkGray)),
            ])
            .into()
        }
        MediaEntry::Show(s) => {
            let watched = s.watched_count();
            let total = s.episode_count();
            let bar = progress_bar(watched, total, 10);
            let bar_style = if watched == total && total > 0 {
                Style::default().fg(Color::Green)
            } else if watched > 0 {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let season_tag = s
                .metadata
                .season
                .as_ref()
                .map(|(n, _)| format!(" S{n:02}"))
                .unwrap_or_default();

            Line::from(vec![
                Span::raw("   "),
                Span::styled(
                    format!("{truncated:<max_title$}"),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(season_tag, Style::default().fg(Color::DarkGray)),
                Span::raw("  "),
                Span::styled(bar, bar_style),
                Span::styled(
                    format!("  {watched}/{total}"),
                    Style::default().fg(Color::DarkGray),
                ),
            ])
            .into()
        }
    }
}

fn draw_footer(f: &mut Frame, app: &App, _n: usize, area: Rect) {
    let content: Line = if app.search_active {
        Line::from(vec![
            Span::styled(
                " / ",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(app.search.as_str(), Style::default().fg(Color::White)),
            Span::styled("█", Style::default().fg(Color::Yellow)),
            Span::styled("   esc to cancel", Style::default().fg(Color::DarkGray)),
        ])
    } else if let Some(msg) = &app.status {
        let style = if msg.is_error {
            Style::default().fg(Color::Red)
        } else {
            Style::default().fg(Color::Green)
        };
        Line::from(Span::styled(format!(" {}", msg.text), style))
    } else {
        Line::from(vec![
            hint("↑↓/jk", "navigate"),
            hint("enter", "open"),
            hint("/", "search"),
            hint("f", "filter"),
            hint("t", "type"),
            hint("s", "sort"),
            hint("q", "quit"),
        ])
    };

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::DarkGray));
    let para = Paragraph::new(content).block(block);
    f.render_widget(para, area);
}

// ── Utilities ─────────────────────────────────────────────────────────────────

fn hint(key: &str, desc: &str) -> Span<'static> {
    let s = format!("  [{key}] {desc}");
    Span::styled(s, Style::default().fg(Color::DarkGray))
}

fn progress_bar(watched: usize, total: usize, width: usize) -> String {
    if total == 0 {
        return "·".repeat(width);
    }
    let filled = (watched * width) / total;
    (0..width)
        .map(|i| if i < filled { '●' } else { '○' })
        .collect()
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let t: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{t}…")
}

fn compute_scroll(selected: usize, visible: usize, total: usize) -> usize {
    if total <= visible {
        return 0;
    }
    if selected < visible / 2 {
        return 0;
    }
    (selected - visible / 2).min(total - visible)
}
