use ratatui::Frame;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};

use crate::paths;
use crate::plan::Selection;
use crate::scan::Detection;

/// Rows above the tools: `apply to all`, `apply to none`.
const ACTIONS: usize = 2;

pub struct SelectScreen {
    tools: Vec<(Detection, bool)>,
    cursor: usize,
}

impl SelectScreen {
    pub fn new(found: Vec<Detection>, exclusions: &[String]) -> Self {
        let tools = found
            .into_iter()
            .map(|d| {
                let checked = !exclusions.contains(&d.port);
                (d, checked)
            })
            .collect();
        Self { tools, cursor: 0 }
    }

    /// Returns true on confirm.
    pub fn handle(&mut self, key: KeyCode) -> bool {
        let last = ACTIONS + self.tools.len() - 1;
        match key {
            KeyCode::Char('j') | KeyCode::Down => self.cursor = (self.cursor + 1).min(last),
            KeyCode::Char('k') | KeyCode::Up => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Char('g') | KeyCode::Home => self.cursor = 0,
            KeyCode::Char('G') | KeyCode::End => self.cursor = last,
            KeyCode::Char(' ') => self.toggle(),
            KeyCode::Enter => return true,
            _ => {}
        }
        false
    }

    pub fn selection(&self) -> Selection {
        let (selected, excluded): (Vec<_>, Vec<_>) =
            self.tools.iter().partition(|(_, checked)| *checked);
        let names = |tools: Vec<&(Detection, bool)>| {
            tools.into_iter().map(|(d, _)| d.port.clone()).collect()
        };
        Selection {
            selected: names(selected),
            excluded: names(excluded),
        }
    }

    fn toggle(&mut self) {
        match self.cursor {
            0 => self.set_all(true),
            1 => self.set_all(false),
            i => self.tools[i - ACTIONS].1 ^= true,
        }
    }

    fn set_all(&mut self, checked: bool) {
        for tool in &mut self.tools {
            tool.1 = checked;
        }
    }

    fn all_checked(&self) -> bool {
        self.tools.iter().all(|(_, c)| *c)
    }

    fn none_checked(&self) -> bool {
        !self.tools.iter().any(|(_, c)| *c)
    }

    pub fn render(&self, frame: &mut Frame) {
        let [title, list, footer] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .margin(1)
        .areas(frame.area());

        let width = self
            .tools
            .iter()
            .map(|(d, _)| d.port.len())
            .max()
            .unwrap_or(0);
        let items = [
            row(self.all_checked(), vec!["apply to all".into()]),
            row(self.none_checked(), vec!["apply to none".into()]),
            ListItem::new(Line::from("─".repeat(width.max(13) + 4)).dim()),
        ]
        .into_iter()
        .chain(self.tools.iter().map(|(d, checked)| {
            row(
                *checked,
                vec![
                    Span::from(format!("{:width$}  ", d.port)),
                    Span::from(paths::display(&d.path)).dim(),
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

        frame.render_widget(Line::from(vec!["vimify".bold(), " select".into()]), title);
        frame.render_stateful_widget(
            List::new(items)
                .highlight_symbol("› ")
                .highlight_style(Style::new().bold()),
            list,
            &mut state,
        );
        frame.render_widget(
            Line::from("j/k move  space toggle  enter confirm  q quit").dim(),
            footer,
        );
    }
}

fn row(checked: bool, label: Vec<Span<'static>>) -> ListItem<'static> {
    let checkbox = if checked { "[x] ".blue() } else { "[ ] ".dim() };
    ListItem::new(Line::from([vec![checkbox], label].concat()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(names: &[&str], exclusions: &[&str]) -> SelectScreen {
        let found = names
            .iter()
            .map(|n| Detection {
                port: n.to_string(),
                path: format!("/bin/{n}").into(),
            })
            .collect();
        let exclusions: Vec<String> = exclusions.iter().map(|e| e.to_string()).collect();
        SelectScreen::new(found, &exclusions)
    }

    fn press(s: &mut SelectScreen, keys: &str) {
        for c in keys.chars() {
            s.handle(KeyCode::Char(c));
        }
    }

    fn selection(selected: &[&str], excluded: &[&str]) -> Selection {
        Selection {
            selected: selected.iter().map(|s| s.to_string()).collect(),
            excluded: excluded.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn exclusions_start_unchecked() {
        let s = screen(&["a", "b", "c"], &["b"]);
        assert_eq!(s.selection(), selection(&["a", "c"], &["b"]));
    }

    #[test]
    fn apply_to_none_then_all() {
        let mut s = screen(&["a", "b"], &[]);
        press(&mut s, "j ");
        assert!(s.none_checked());
        press(&mut s, "k ");
        assert!(s.all_checked());
    }

    #[test]
    fn toggling_a_tool_updates_action_rows() {
        let mut s = screen(&["a", "b"], &[]);
        press(&mut s, "jj ");
        assert_eq!(s.selection(), selection(&["b"], &["a"]));
        assert!(!s.all_checked() && !s.none_checked());
    }

    #[test]
    fn cursor_stays_in_bounds() {
        let mut s = screen(&["a"], &[]);
        press(&mut s, "kkk");
        assert_eq!(s.cursor, 0);
        press(&mut s, "jjjjj");
        assert_eq!(s.cursor, 2);
        press(&mut s, "g");
        assert_eq!(s.cursor, 0);
        press(&mut s, "G");
        assert_eq!(s.cursor, 2);
    }

    #[test]
    fn enter_confirms() {
        let mut s = screen(&["a"], &[]);
        assert!(!s.handle(KeyCode::Char('j')));
        assert!(s.handle(KeyCode::Enter));
    }
}
