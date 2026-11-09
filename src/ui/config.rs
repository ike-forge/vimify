use ratatui::Frame;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{HighlightSpacing, List, ListItem, ListState, Paragraph, Wrap};

use crate::reference::Reference;
use crate::{actions, palette};

/// Fits `catppuccin-frappe` and the edit cursor.
pub const WIDTH: u16 = 30;

/// Edits are kept here until the user accepts the plan, which saves them.
pub struct ConfigPane {
    original: Reference,
    reference: Reference,
    /// Row 0 is the theme, then one row per action.
    cursor: usize,
    input: Option<String>,
    error: Option<String>,
}

impl ConfigPane {
    pub fn new(reference: Reference) -> Self {
        Self {
            original: reference.clone(),
            reference,
            cursor: 0,
            input: None,
            error: None,
        }
    }

    pub fn reference(&self) -> &Reference {
        &self.reference
    }

    pub fn editing(&self) -> bool {
        self.input.is_some()
    }

    pub fn handle(&mut self, key: KeyCode) {
        if let Some(input) = &mut self.input {
            match key {
                KeyCode::Char(c) => input.push(c),
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Enter => self.commit(),
                KeyCode::Esc => {
                    self.input = None;
                    self.error = None;
                }
                _ => {}
            }
            return;
        }
        let last = actions::DEFAULTS.len();
        match key {
            KeyCode::Char('j') | KeyCode::Down => self.cursor = (self.cursor + 1).min(last),
            KeyCode::Char('k') | KeyCode::Up => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Char('g') | KeyCode::Home => self.cursor = 0,
            KeyCode::Char('G') | KeyCode::End => self.cursor = last,
            KeyCode::Enter => {
                self.input = Some(value(&self.reference, self.cursor).to_owned());
                self.error = None;
            }
            _ => {}
        }
    }

    /// An invalid value keeps the input open with the error below it.
    fn commit(&mut self) {
        let Some(input) = self.input.take() else {
            return;
        };
        let text = input.trim().to_owned();
        let check = if self.cursor == 0 {
            if palette::names().contains(&text.as_str()) {
                Ok(())
            } else {
                Err(format!(
                    "Unknown theme. Known: {}",
                    palette::names().join(", ")
                ))
            }
        } else if actions::is_key(&text) {
            Ok(())
        } else {
            Err("One character, or <C-u> style.".to_owned())
        };
        match check {
            Ok(()) => {
                self.error = None;
                match self.cursor {
                    0 => self.reference.theme = text,
                    i => {
                        let action = actions::DEFAULTS[i - 1].0;
                        self.reference.keys.insert(action.to_owned(), text);
                    }
                }
            }
            Err(e) => {
                self.error = Some(e);
                self.input = Some(input);
            }
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, focused: bool) {
        let [header, list, error] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(3),
        ])
        .areas(area);
        frame.render_widget(heading("Config", focused), header);

        let items = (0..=actions::DEFAULTS.len()).map(|i| {
            let label = format!("{:8}", name(i)).dim();
            let shown = match &self.input {
                Some(input) if i == self.cursor => {
                    return ListItem::new(Line::from(vec![
                        label,
                        input.clone().into(),
                        " ".reversed(),
                    ]));
                }
                _ => value(&self.reference, i),
            };
            let value = if shown == value(&self.original, i) {
                Span::from(shown.to_owned())
            } else {
                Span::from(format!("{shown} *")).yellow()
            };
            ListItem::new(Line::from(vec![label, value]))
        });
        let mut state = ListState::default().with_selected(Some(self.cursor));
        frame.render_stateful_widget(cursor_list(items.collect(), focused), list, &mut state);
        if let Some(e) = &self.error {
            frame.render_widget(
                Paragraph::new(e.as_str()).red().wrap(Wrap { trim: true }),
                error,
            );
        }
    }
}

fn name(row: usize) -> &'static str {
    match row {
        0 => "theme",
        i => actions::DEFAULTS[i - 1].0,
    }
}

fn value(reference: &Reference, row: usize) -> &str {
    match row {
        0 => &reference.theme,
        i => reference
            .keys
            .get(actions::DEFAULTS[i - 1].0)
            .map_or("", String::as_str),
    }
}

pub fn heading(text: &str, focused: bool) -> Line<'static> {
    let style = if focused {
        Style::new().bold().underlined()
    } else {
        Style::new().dim()
    };
    Line::styled(text.to_owned(), style)
}

/// Only the focused pane shows its cursor; the spacing keeps columns still on a switch.
pub fn cursor_list(items: Vec<ListItem<'static>>, focused: bool) -> List<'static> {
    let list = List::new(items).highlight_spacing(HighlightSpacing::Always);
    if focused {
        list.highlight_symbol("› ")
            .highlight_style(Style::new().bold())
    } else {
        list.highlight_symbol("  ").style(Style::new().dim())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(pane: &mut ConfigPane, keys: &str) {
        for c in keys.chars() {
            pane.handle(KeyCode::Char(c));
        }
    }

    fn type_value(pane: &mut ConfigPane, value: &str) {
        pane.handle(KeyCode::Enter);
        for _ in 0..30 {
            pane.handle(KeyCode::Backspace);
        }
        press(pane, value);
        pane.handle(KeyCode::Enter);
    }

    #[test]
    fn edits_a_key() {
        let mut pane = ConfigPane::new(Reference::default());
        press(&mut pane, "jj");
        type_value(&mut pane, "<C-n>");
        assert!(!pane.editing());
        assert_eq!(pane.reference().keys["down"], "<C-n>");
    }

    #[test]
    fn invalid_key_keeps_the_input_open() {
        let mut pane = ConfigPane::new(Reference::default());
        press(&mut pane, "j");
        type_value(&mut pane, "ab");
        assert!(pane.editing());
        assert!(pane.error.is_some());
        assert_eq!(pane.reference().keys["left"], "h");
    }

    #[test]
    fn unknown_theme_is_rejected() {
        let mut pane = ConfigPane::new(Reference::default());
        type_value(&mut pane, "nope");
        assert!(pane.editing());
        assert_eq!(pane.reference().theme, Reference::default().theme);
    }

    #[test]
    fn esc_discards_the_input() {
        let mut pane = ConfigPane::new(Reference::default());
        press(&mut pane, "jj");
        pane.handle(KeyCode::Enter);
        press(&mut pane, "q");
        pane.handle(KeyCode::Esc);
        assert!(!pane.editing());
        assert_eq!(pane.reference().keys["down"], "j");
    }

    #[test]
    fn typing_does_not_move_the_cursor() {
        let mut pane = ConfigPane::new(Reference::default());
        pane.handle(KeyCode::Enter);
        press(&mut pane, "jjj");
        assert_eq!(pane.cursor, 0);
    }
}
