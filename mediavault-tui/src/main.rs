mod app;
mod input;
mod screens;
mod ui;

use crossterm::event::KeyEventKind;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use app::{Action, App};

fn main() -> anyhow::Result<()> {
    // Parse --library / -l flag manually to avoid pulling in clap
    let args: Vec<String> = std::env::args().collect();
    let explicit = args
        .iter()
        .position(|a| a == "--library" || a == "-l")
        .and_then(|pos| args.get(pos + 1))
        .map(PathBuf::from);
    let library = mediavault_core::resolve_library(explicit).map_err(|e| {
        anyhow::anyhow!("{e}\nrun mediavault-tui from your media folder, or pass --library <path>")
    })?;
    let entries = mediavault_core::scan_library(&library);

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
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
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
