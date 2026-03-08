use crate::app::{App, Screen};
use crate::screens::{detail, library};
use ratatui::Frame;

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    match app.screen {
        Screen::Library => library::draw(f, app, area),
        Screen::Detail => detail::draw(f, app, area),
    }
}
