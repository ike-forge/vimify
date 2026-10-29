mod scan;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use crate::port::Port;
use scan::ScanScreen;

pub fn run(dirs: Vec<PathBuf>, ports: Vec<Port>) -> Result<()> {
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, dirs, ports);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut DefaultTerminal, dirs: Vec<PathBuf>, ports: Vec<Port>) -> Result<()> {
    let mut screen = ScanScreen::new(dirs.len());
    let rx = crate::scan::spawn(dirs, ports);
    loop {
        while let Ok(ev) = rx.try_recv() {
            screen.update(ev);
        }
        terminal.draw(|f| screen.render(f))?;
        if event::poll(Duration::from_millis(50))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            let ctrl_c =
                key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
            if ctrl_c || matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                return Ok(());
            }
        }
    }
}
