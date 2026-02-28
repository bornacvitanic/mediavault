mod app;
mod screens;
mod input;
mod ui;

use std::io;
use std::path::PathBuf;
use std::time::Duration;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use crossterm::event::KeyEventKind;
use ratatui::{backend::CrosstermBackend, Terminal};

use app::{App, Action};

fn main() -> anyhow::Result<()> {
    let library = resolve_library()?;
    let entries = media_core::scan_library(&library);

    // ── Terminal setup ────────────────────────────────────────────────────────
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // ── Run ───────────────────────────────────────────────────────────────────
    let mut app = App::new(entries);
    let result = run(&mut terminal, &mut app);

    // ── Restore terminal ──────────────────────────────────────────────────────
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
    Ok(())
}

fn run<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> anyhow::Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        // Poll with a short timeout so status messages can auto-expire.
        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                let action = input::map_key(key, app);
                if matches!(action, Action::Quit) {
                    return Ok(());
                }
                app.handle(action);
            }
        } else {
            // Tick — expire status messages.
            app.tick();
        }
    }
}

// ── Library resolution ────────────────────────────────────────────────────────

fn resolve_library() -> anyhow::Result<PathBuf> {
    // 1. --library / -l flag (simple manual parse — avoids pulling in clap)
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--library" || a == "-l") {
        if let Some(p) = args.get(pos + 1) {
            let path = PathBuf::from(p);
            if path.is_dir() { return Ok(path); }
            anyhow::bail!("{} is not a directory", p);
        }
    }

    // 2. Current directory if it looks like a media folder
    let cwd = std::env::current_dir()?;
    if looks_like_media_dir(&cwd) {
        return Ok(cwd);
    }

    // 3. Saved config path (shared with GUI and CLI)
    let config = media_core::tmdb::load_config();
    if !config.library_path.is_empty() {
        let p = PathBuf::from(&config.library_path);
        if p.is_dir() { return Ok(p); }
    }

    anyhow::bail!(
        "could not find a media library\n\
         run mvt from your media folder, or pass --library <path>"
    )
}

fn looks_like_media_dir(dir: &std::path::Path) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else { return false; };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_file() && media_core::is_video(&p) { return true; }
        if p.is_dir() {
            if let Ok(sub) = std::fs::read_dir(&p) {
                if sub.flatten().any(|e| {
                    let sp = e.path();
                    sp.is_file() && media_core::is_video(&sp)
                }) { return true; }
            }
        }
    }
    false
}