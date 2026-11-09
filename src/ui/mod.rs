mod config;
mod confirm;
mod rollback;
mod scan;
mod select;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{HighlightSpacing, List, ListItem};
use ratatui::{DefaultTerminal, Frame};

use crate::plan::{self, Op, Selection};
use crate::port::Port;
use crate::reference::Reference;
use crate::state::State;
use config::ConfigScreen;
use confirm::{Choice, ConfirmScreen};
use rollback::RollbackScreen;
use scan::ScanScreen;
use select::SelectScreen;

/// What the user accepted on the confirm screen.
pub struct Outcome {
    pub ops: Vec<Op>,
    pub exclusions: Vec<String>,
    /// The port each detected tool resolved to.
    pub ports: Vec<Port>,
}

/// A scan that finishes within this window never shows its progress screen.
const SCAN_SCREEN_DELAY: Duration = Duration::from_millis(400);

/// The picker before confirm: `back` restores it with the user's checkboxes.
enum Picker {
    Select(SelectScreen),
    Rollback(RollbackScreen),
}

impl Picker {
    fn render(&self, frame: &mut Frame) {
        match self {
            Picker::Select(s) => s.render(frame),
            Picker::Rollback(r) => r.render(frame),
        }
    }

    fn captures_input(&self) -> bool {
        matches!(self, Picker::Select(s) if s.captures_input())
    }

    fn handle(&mut self, key: KeyCode) -> bool {
        match self {
            Picker::Select(s) => s.handle(key),
            Picker::Rollback(r) => r.handle(key),
        }
    }

    fn ports(&self) -> Vec<Port> {
        match self {
            Picker::Select(s) => s.ports(),
            Picker::Rollback(_) => Vec::new(),
        }
    }
}

enum Screen {
    Scan(ScanScreen),
    Pick(Picker),
    Confirm(Picker, ConfirmScreen),
}

/// `None` if the user quit.
pub fn apply(
    dirs: Vec<PathBuf>,
    ports: Vec<Port>,
    reference: &Reference,
    state: &State,
) -> Result<Option<Outcome>> {
    let hash = reference.hash()?;
    let rx = crate::scan::spawn(dirs.clone(), ports.clone());
    let started = Instant::now();
    let screen = Screen::Scan(ScanScreen::new(dirs.len()));
    with_terminal(|terminal| {
        run(
            terminal,
            screen,
            |screen| {
                if let Screen::Scan(scan) = screen {
                    while let Ok(ev) = rx.try_recv() {
                        scan.update(ev);
                    }
                    if scan.finished() && !scan.found().is_empty() {
                        let found = scan.found().to_vec();
                        let select = SelectScreen::new(found, &ports, &reference.exclusions);
                        *screen = Screen::Pick(Picker::Select(select));
                    }
                }
                started.elapsed() >= SCAN_SCREEN_DELAY
            },
            |picker| match picker {
                Picker::Select(s) => {
                    let selection = s.selection();
                    ConfirmScreen {
                        ops: plan::plan(&selection, &s.ports(), state, &hash),
                        exclusions: plan::exclusions(&reference.exclusions, &selection),
                    }
                }
                Picker::Rollback(_) => unreachable!("apply never shows the rollback picker"),
            },
        )
    })
}

/// `None` if the user quit or no tool is applied.
pub fn rollback(state: &State, exclusions: &[String]) -> Result<Option<Outcome>> {
    let screen = Screen::Pick(Picker::Rollback(RollbackScreen::new(state)));
    with_terminal(|terminal| {
        run(
            terminal,
            screen,
            |_| true,
            |picker| {
                let checked = match picker {
                    Picker::Rollback(r) => r.checked(),
                    Picker::Select(_) => unreachable!("rollback never shows the select picker"),
                };
                let selection = Selection {
                    selected: Vec::new(),
                    excluded: checked,
                };
                ConfirmScreen {
                    ops: plan::plan(&selection, &[], state, ""),
                    exclusions: plan::exclusions(exclusions, &selection),
                }
            },
        )
    })
}

/// The edited reference on save, `None` if the user quit.
pub fn config(reference: &Reference) -> Result<Option<Reference>> {
    let mut screen = ConfigScreen::new(reference.clone());
    with_terminal(|terminal| {
        loop {
            terminal.draw(|f| screen.render(f))?;
            let Some(key) = next_key()? else {
                continue;
            };
            let quit = matches!(key.code, KeyCode::Char('q') | KeyCode::Esc);
            if is_ctrl_c(&key) || (quit && !screen.editing()) {
                return Ok(None);
            }
            if screen.handle(key.code) {
                return Ok(Some(screen.reference().clone()));
            }
        }
    })
}

fn with_terminal<T>(body: impl FnOnce(&mut DefaultTerminal) -> Result<T>) -> Result<T> {
    let mut terminal = ratatui::init();
    let result = body(&mut terminal);
    ratatui::restore();
    result
}

/// `tick` advances the scan and says whether to draw it yet; `confirm` plans the
/// picker's choice.
fn run(
    terminal: &mut DefaultTerminal,
    mut screen: Screen,
    mut tick: impl FnMut(&mut Screen) -> bool,
    confirm: impl Fn(&Picker) -> ConfirmScreen,
) -> Result<Option<Outcome>> {
    loop {
        let show_scan = tick(&mut screen);
        terminal.draw(|f| match &screen {
            Screen::Scan(s) => {
                if show_scan {
                    s.render(f);
                }
            }
            Screen::Pick(p) => p.render(f),
            Screen::Confirm(_, c) => c.render(f),
        })?;
        let Some(key) = next_key()? else {
            continue;
        };
        let esc_quits = !matches!(screen, Screen::Confirm(..));
        // `q` and Esc close the modal first.
        let modal = matches!(&screen, Screen::Pick(p) if p.captures_input());
        let quit = key.code == KeyCode::Char('q') || (esc_quits && key.code == KeyCode::Esc);
        if is_ctrl_c(&key) || (quit && !modal) {
            return Ok(None);
        }
        screen = match screen {
            Screen::Pick(mut p) => {
                if p.handle(key.code) {
                    let c = confirm(&p);
                    Screen::Confirm(p, c)
                } else {
                    Screen::Pick(p)
                }
            }
            Screen::Confirm(p, c) => match c.handle(key.code) {
                Some(Choice::Accept) => {
                    return Ok(Some(Outcome {
                        ops: c.ops,
                        exclusions: c.exclusions,
                        ports: p.ports(),
                    }));
                }
                Some(Choice::Back) => Screen::Pick(p),
                None => Screen::Confirm(p, c),
            },
            scan @ Screen::Scan(_) => scan,
        };
    }
}

/// A key press, or `None` when none arrived within the redraw interval.
fn next_key() -> Result<Option<KeyEvent>> {
    if !event::poll(Duration::from_millis(50))? {
        return Ok(None);
    }
    match event::read()? {
        Event::Key(key) if key.kind == KeyEventKind::Press => Ok(Some(key)),
        _ => Ok(None),
    }
}

fn is_ctrl_c(key: &KeyEvent) -> bool {
    key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)
}

fn cursor_list(items: Vec<ListItem<'static>>) -> List<'static> {
    List::new(items)
        .highlight_spacing(HighlightSpacing::Always)
        .highlight_symbol("› ")
        .highlight_style(Style::new().bold())
}

fn row(checked: bool, label: Vec<Span<'static>>) -> ListItem<'static> {
    let checkbox = if checked { "[x] ".blue() } else { "[ ] ".dim() };
    ListItem::new(Line::from([vec![checkbox], label].concat()))
}

#[cfg(test)]
pub(crate) mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;

    pub fn draw(width: u16, height: u16, render: impl FnOnce(&mut Frame)) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(render).unwrap();
        let buffer = terminal.backend().buffer();
        buffer
            .content()
            .chunks(buffer.area.width as usize)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>() + "\n")
            .collect()
    }
}
