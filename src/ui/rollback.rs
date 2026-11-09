use ratatui::Frame;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{ListItem, ListState};

use super::{cursor_list, row};
use crate::paths;
use crate::state::State;

/// Rows above the tools: `Revert changes`, `select all`, `select none`.
const ACTIONS: usize = 3;

struct Tool {
    name: String,
    path: String,
    checked: bool,
}

/// Applied tools, all unchecked: a check marks a tool for revert.
pub struct RollbackScreen {
    tools: Vec<Tool>,
    cursor: usize,
}

impl RollbackScreen {
    pub fn new(state: &State) -> Self {
        let tools = state
            .tools
            .iter()
            .map(|(name, applied)| Tool {
                name: name.clone(),
                path: applied
                    .files
                    .first()
                    .map(|f| paths::display(&f.path))
                    .unwrap_or_default(),
                checked: false,
            })
            .collect();
        Self { tools, cursor: 0 }
    }

    pub fn checked(&self) -> Vec<String> {
        self.tools
            .iter()
            .filter(|t| t.checked)
            .map(|t| t.name.clone())
            .collect()
    }

    /// Returns true on confirm.
    pub fn handle(&mut self, key: KeyCode) -> bool {
        let last = ACTIONS + self.tools.len() - 1;
        match key {
            KeyCode::Enter if self.cursor == 0 => return true,
            KeyCode::Char('j') | KeyCode::Down => self.cursor = (self.cursor + 1).min(last),
            KeyCode::Char('k') | KeyCode::Up => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Char('g') | KeyCode::Home => self.cursor = 0,
            KeyCode::Char('G') | KeyCode::End => self.cursor = last,
            KeyCode::Char(' ') | KeyCode::Enter => match self.cursor {
                0 => {}
                1 => self.set_all(true),
                2 => self.set_all(false),
                i => self.tools[i - ACTIONS].checked ^= true,
            },
            _ => {}
        }
        false
    }

    fn set_all(&mut self, checked: bool) {
        for tool in &mut self.tools {
            tool.checked = checked;
        }
    }

    pub fn render(&self, frame: &mut Frame) {
        let [title, list, footer] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .margin(1)
        .areas(frame.area());

        let width = self.tools.iter().map(|t| t.name.len()).max().unwrap_or(0);
        let all = self.tools.iter().all(|t| t.checked);
        let none = !self.tools.iter().any(|t| t.checked);
        let items = [
            ListItem::new(Line::from(" Revert changes ".reversed())),
            row(all, vec!["select all".into()]),
            row(none, vec!["select none".into()]),
            ListItem::new(Line::from("─".repeat(width.max("select none".len()) + 4)).dim()),
        ]
        .into_iter()
        .chain(self.tools.iter().map(|tool| {
            row(
                tool.checked,
                vec![
                    Span::from(format!("{:width$}  ", tool.name)),
                    Span::from(tool.path.clone()).dim(),
                ],
            )
        }));
        // The separator occupies a list index but is not selectable.
        let index = if self.cursor < ACTIONS {
            self.cursor
        } else {
            self.cursor + 1
        };
        let mut state = ListState::default().with_selected(Some(index));

        frame.render_widget(Line::from(vec!["vimify".bold(), " rollback".into()]), title);
        frame.render_stateful_widget(cursor_list(items.collect()), list, &mut state);
        frame.render_widget(Line::from("space mark for revert  q quit").dim(), footer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Applied;
    use crate::ui::tests::draw;

    fn screen(names: &[&str]) -> RollbackScreen {
        let mut state = State::default();
        for name in names {
            state.tools.insert(
                name.to_string(),
                Applied {
                    port_version: "1".into(),
                    reference_hash: String::new(),
                    files: Vec::new(),
                },
            );
        }
        RollbackScreen::new(&state)
    }

    fn press(s: &mut RollbackScreen, keys: &str) {
        for c in keys.chars() {
            s.handle(KeyCode::Char(c));
        }
    }

    #[test]
    fn tools_start_unchecked() {
        let s = screen(&["a", "b"]);
        assert!(s.checked().is_empty());
    }

    #[test]
    fn space_marks_a_tool() {
        let mut s = screen(&["a", "b"]);
        press(&mut s, "G ");
        assert_eq!(s.checked(), ["b"]);
    }

    #[test]
    fn select_all_then_none() {
        let mut s = screen(&["a", "b"]);
        press(&mut s, "j ");
        assert_eq!(s.checked(), ["a", "b"]);
        press(&mut s, "j ");
        assert!(s.checked().is_empty());
    }

    #[test]
    fn enter_confirms_on_the_button_and_toggles_elsewhere() {
        let mut s = screen(&["a"]);
        press(&mut s, "G");
        assert!(!s.handle(KeyCode::Enter));
        assert_eq!(s.checked(), ["a"]);
        press(&mut s, "g");
        assert!(s.handle(KeyCode::Enter));
    }

    #[test]
    fn lists_applied_tools() {
        let s = screen(&["lazygit", "yazi"]);
        let shown = draw(60, 12, |f| s.render(f));
        assert!(shown.contains("[ ] lazygit"), "{shown}");
        assert!(shown.contains("[ ] yazi"), "{shown}");
    }
}
