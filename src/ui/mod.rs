mod scan;
mod select;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::port::Port;
use scan::ScanScreen;
use select::SelectScreen;
pub use select::Selection;

enum Screen {
    Scan(ScanScreen),
    Select(SelectScreen),
}

/// `None` if the user quit.
pub fn run(
    dirs: Vec<PathBuf>,
    ports: Vec<Port>,
    exclusions: &[String],
) -> Result<Option<Selection>> {
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, dirs, ports, exclusions);
    ratatui::restore();
    result
}

fn event_loop(
    terminal: &mut DefaultTerminal,
    dirs: Vec<PathBuf>,
    ports: Vec<Port>,
    exclusions: &[String],
) -> Result<Option<Selection>> {
    let mut screen = Screen::Scan(ScanScreen::new(dirs.len()));
    let rx = crate::scan::spawn(dirs, ports);
    loop {
        if let Screen::Scan(scan) = &mut screen {
            while let Ok(ev) = rx.try_recv() {
                scan.update(ev);
            }
            if scan.finished() && !scan.found().is_empty() {
                screen = Screen::Select(SelectScreen::new(scan.found().to_vec(), exclusions));
            }
        }
        terminal.draw(|f| match &screen {
            Screen::Scan(s) => s.render(f),
            Screen::Select(s) => s.render(f),
        })?;
        if !event::poll(Duration::from_millis(50))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if is_quit(&key) {
            return Ok(None);
        }
        if let Screen::Select(s) = &mut screen
            && s.handle(key.code)
        {
            return Ok(Some(s.selection()));
        }
    }
}

fn is_quit(key: &KeyEvent) -> bool {
    let ctrl_c = key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
    ctrl_c || matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
}
