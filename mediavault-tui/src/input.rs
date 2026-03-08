use crate::app::{Action, App, Screen};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Map a raw key event to a semantic Action, taking current screen/mode into account.
pub fn map_key(key: KeyEvent, app: &App) -> Action {
    // Ctrl+C always quits
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Action::Quit;
    }

    // In search mode, most keys are characters
    if app.search_active {
        return match key.code {
            KeyCode::Esc => Action::Escape,
            KeyCode::Backspace => Action::Backspace,
            KeyCode::Enter => Action::Select,
            KeyCode::Up => Action::Up,
            KeyCode::Down => Action::Down,
            KeyCode::Char(c) => Action::Char(c),
            _ => Action::Noop,
        };
    }

    match app.screen {
        Screen::Library => match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => Action::Quit,
            KeyCode::Up | KeyCode::Char('k') => Action::Up,
            KeyCode::Down | KeyCode::Char('j') => Action::Down,
            KeyCode::PageUp | KeyCode::Char('K') => Action::PageUp,
            KeyCode::PageDown | KeyCode::Char('J') => Action::PageDown,
            KeyCode::Enter | KeyCode::Right => Action::Select,
            KeyCode::Char('f') => Action::CycleFilter,
            KeyCode::Char('t') => Action::CycleKind,
            KeyCode::Char('s') => Action::CycleSort,
            KeyCode::Char('/') => Action::SearchMode,
            _ => Action::Noop,
        },
        Screen::Detail => match key.code {
            KeyCode::Char('q') => Action::Quit,
            KeyCode::Esc | KeyCode::Left | KeyCode::Backspace | KeyCode::Char('h') => Action::Back,
            KeyCode::Up | KeyCode::Char('k') => Action::Up,
            KeyCode::Down | KeyCode::Char('j') => Action::Down,
            KeyCode::PageUp | KeyCode::Char('K') => Action::PageUp,
            KeyCode::PageDown | KeyCode::Char('J') => Action::PageDown,
            KeyCode::Enter | KeyCode::Char('p') => Action::Play,
            KeyCode::Char(' ') => Action::ToggleWatched,
            KeyCode::Char('d') => Action::ToggleWatched,
            KeyCode::Char('a') => Action::MarkAllWatched,
            KeyCode::Char('F') => Action::FetchSubs,
            KeyCode::Char('n') => Action::Notes,
            _ => Action::Noop,
        },
    }
}
