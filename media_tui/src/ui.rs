use ratatui::Frame;
use crate::app::{App, Screen};
use crate::screens::{library, detail};

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    match app.screen {
        Screen::Library => library::draw(f, app, area),
        Screen::Detail  => detail::draw(f, app, area),
    }
}
