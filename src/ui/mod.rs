mod confirm;
mod scan;
mod select;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::plan::{self, Op};
use crate::port::Port;
use crate::state::State;
use confirm::{Choice, ConfirmScreen};
use scan::ScanScreen;
use select::SelectScreen;

/// What the user accepted on the confirm screen.
pub struct Outcome {
    pub ops: Vec<Op>,
    pub exclusions: Vec<String>,
}

enum Screen {
    Scan(ScanScreen),
    Select(SelectScreen),
    /// Keeps the select screen so `back` restores the user's checkboxes.
    Confirm(SelectScreen, ConfirmScreen),
}

/// `None` if the user quit.
pub fn run(
    dirs: Vec<PathBuf>,
    ports: Vec<Port>,
    exclusions: &[String],
    state: &State,
) -> Result<Option<Outcome>> {
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, dirs, ports, exclusions, state);
    ratatui::restore();
    result
}

fn event_loop(
    terminal: &mut DefaultTerminal,
    dirs: Vec<PathBuf>,
    ports: Vec<Port>,
    exclusions: &[String],
    state: &State,
) -> Result<Option<Outcome>> {
    let mut screen = Screen::Scan(ScanScreen::new(dirs.len()));
    let rx = crate::scan::spawn(dirs, ports.clone());
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
            Screen::Confirm(_, c) => c.render(f),
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
        let esc_quits = !matches!(screen, Screen::Confirm(..));
        if is_quit(&key) || (esc_quits && key.code == KeyCode::Esc) {
            return Ok(None);
        }
        screen = match screen {
            Screen::Select(mut s) => {
                if s.handle(key.code) {
                    let selection = s.selection();
                    let confirm = ConfirmScreen {
                        ops: plan::plan(&selection, &ports, state),
                        exclusions: plan::exclusions(exclusions, &selection),
                    };
                    Screen::Confirm(s, confirm)
                } else {
                    Screen::Select(s)
                }
            }
            Screen::Confirm(s, c) => match c.handle(key.code) {
                Some(Choice::Accept) => {
                    return Ok(Some(Outcome {
                        ops: c.ops,
                        exclusions: c.exclusions,
                    }));
                }
                Some(Choice::Back) => Screen::Select(s),
                None => Screen::Confirm(s, c),
            },
            scan @ Screen::Scan(_) => scan,
        };
    }
}

fn is_quit(key: &KeyEvent) -> bool {
    let ctrl_c = key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
    ctrl_c || key.code == KeyCode::Char('q')
}
