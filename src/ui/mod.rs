mod config;
mod confirm;
mod scan;
mod select;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::plan::{self, Op};
use crate::port::Port;
use crate::reference::Reference;
use crate::state::State;
use confirm::{Choice, ConfirmScreen};
use scan::ScanScreen;
use select::SelectScreen;

/// What the user accepted on the confirm screen.
pub struct Outcome {
    pub ops: Vec<Op>,
    pub exclusions: Vec<String>,
    /// The port each detected tool resolved to.
    pub ports: Vec<Port>,
    /// With the user's edits from the config pane, not yet saved.
    pub reference: Reference,
}

/// A scan that finishes within this window never shows its progress screen.
const SCAN_SCREEN_DELAY: Duration = Duration::from_millis(400);

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
    reference: &Reference,
    state: &State,
) -> Result<Option<Outcome>> {
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, dirs, ports, reference, state);
    ratatui::restore();
    result
}

fn event_loop(
    terminal: &mut DefaultTerminal,
    dirs: Vec<PathBuf>,
    ports: Vec<Port>,
    reference: &Reference,
    state: &State,
) -> Result<Option<Outcome>> {
    let mut screen = Screen::Scan(ScanScreen::new(dirs.len()));
    let rx = crate::scan::spawn(dirs, ports.clone());
    let started = Instant::now();
    loop {
        if let Screen::Scan(scan) = &mut screen {
            while let Ok(ev) = rx.try_recv() {
                scan.update(ev);
            }
            if scan.finished() && !scan.found().is_empty() {
                screen = Screen::Select(SelectScreen::new(
                    scan.found().to_vec(),
                    &ports,
                    reference.clone(),
                ));
            }
        }
        terminal.draw(|f| match &screen {
            Screen::Scan(s) => {
                if started.elapsed() >= SCAN_SCREEN_DELAY {
                    s.render(f);
                }
            }
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
        // `q` and Esc close the modal first.
        let modal = matches!(&screen, Screen::Select(s) if s.captures_input());
        let quit = key.code == KeyCode::Char('q') || (esc_quits && key.code == KeyCode::Esc);
        if is_ctrl_c(&key) || (quit && !modal) {
            return Ok(None);
        }
        screen = match screen {
            Screen::Select(mut s) => {
                if s.handle(key.code) {
                    let selection = s.selection();
                    let confirm = ConfirmScreen {
                        ops: plan::plan(&selection, &s.ports(), state, &s.reference().hash()?),
                        exclusions: plan::exclusions(&reference.exclusions, &selection),
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
                        ports: s.ports(),
                        reference: s.reference().clone(),
                    }));
                }
                Some(Choice::Back) => Screen::Select(s),
                None => Screen::Confirm(s, c),
            },
            scan @ Screen::Scan(_) => scan,
        };
    }
}

fn is_ctrl_c(key: &KeyEvent) -> bool {
    key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)
}
