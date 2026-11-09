use ratatui::Frame;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{ListItem, ListState, Paragraph, Wrap};

use super::cursor_list;
use crate::reference::Reference;
use crate::{actions, palette};

/// Edits are kept here until `Save changes`.
pub struct ConfigScreen {
    original: Reference,
    reference: Reference,
    /// Row 0 is `Save changes`, row 1 the theme, then one row per action.
    cursor: usize,
    input: Option<String>,
    error: Option<String>,
}

impl ConfigScreen {
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

    /// A text input takes every key, `q` and Esc included.
    pub fn editing(&self) -> bool {
        self.input.is_some()
    }

    /// Returns true on save.
    pub fn handle(&mut self, key: KeyCode) -> bool {
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
            return false;
        }
        let last = actions::DEFAULTS.len() + 1;
        match key {
            KeyCode::Char('j') | KeyCode::Down => self.cursor = (self.cursor + 1).min(last),
            KeyCode::Char('k') | KeyCode::Up => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Char('g') | KeyCode::Home => self.cursor = 0,
            KeyCode::Char('G') | KeyCode::End => self.cursor = last,
            KeyCode::Enter if self.cursor == 0 => return true,
            KeyCode::Enter => {
                self.input = Some(value(&self.reference, self.cursor - 1).to_owned());
                self.error = None;
            }
            _ => {}
        }
        false
    }

    /// An invalid value keeps the input open with the error below it.
    fn commit(&mut self) {
        let Some(input) = self.input.take() else {
            return;
        };
        let text = input.trim().to_owned();
        let field = self.cursor - 1;
        let check = if field == 0 {
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
                match field {
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

    pub fn render(&self, frame: &mut Frame) {
        let fields = actions::DEFAULTS.len() + 1;
        let [title, list, error, footer] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(fields as u16 + 1),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .margin(1)
        .areas(frame.area());

        let fields = (0..fields).map(|i| {
            let label = format!("{:8}", name(i)).dim();
            let shown = match &self.input {
                Some(input) if i + 1 == self.cursor => {
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
        let items = std::iter::once(ListItem::new(Line::from(" Save changes ".reversed())))
            .chain(fields)
            .collect();
        let mut state = ListState::default().with_selected(Some(self.cursor));

        frame.render_widget(Line::from(vec!["vimify".bold(), " config".into()]), title);
        frame.render_stateful_widget(cursor_list(items), list, &mut state);
        if let Some(e) = &self.error {
            frame.render_widget(
                Paragraph::new(e.as_str()).red().wrap(Wrap { trim: true }),
                error,
            );
        }
        let hint = if self.editing() {
            "enter accept  esc cancel"
        } else {
            "enter edit  q quit without saving"
        };
        frame.render_widget(Line::from(hint).dim(), footer);
    }
}

/// `field` 0 is the theme, then one per action.
fn name(field: usize) -> &'static str {
    match field {
        0 => "theme",
        i => actions::DEFAULTS[i - 1].0,
    }
}

fn value(reference: &Reference, field: usize) -> &str {
    match field {
        0 => &reference.theme,
        i => reference
            .keys
            .get(actions::DEFAULTS[i - 1].0)
            .map_or("", String::as_str),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::tests::draw;

    fn press(screen: &mut ConfigScreen, keys: &str) {
        for c in keys.chars() {
            screen.handle(KeyCode::Char(c));
        }
    }

    fn type_value(screen: &mut ConfigScreen, value: &str) {
        screen.handle(KeyCode::Enter);
        for _ in 0..30 {
            screen.handle(KeyCode::Backspace);
        }
        press(screen, value);
        screen.handle(KeyCode::Enter);
    }

    #[test]
    fn enter_saves_on_the_button_and_edits_elsewhere() {
        let mut screen = ConfigScreen::new(Reference::default());
        assert!(screen.handle(KeyCode::Enter));
        press(&mut screen, "j");
        assert!(!screen.handle(KeyCode::Enter));
        assert!(screen.editing());
    }

    #[test]
    fn edits_a_key() {
        let mut screen = ConfigScreen::new(Reference::default());
        press(&mut screen, "jjj");
        type_value(&mut screen, "<C-n>");
        assert!(!screen.editing());
        assert_eq!(screen.reference().keys["down"], "<C-n>");
    }

    #[test]
    fn edits_the_theme() {
        let mut screen = ConfigScreen::new(Reference::default());
        press(&mut screen, "j");
        type_value(&mut screen, "solarized-dark");
        assert_eq!(screen.reference().theme, "solarized-dark");
        assert!(draw(80, 20, |f| screen.render(f)).contains("solarized-dark *"));
    }

    #[test]
    fn invalid_key_keeps_the_input_open() {
        let mut screen = ConfigScreen::new(Reference::default());
        press(&mut screen, "jj");
        type_value(&mut screen, "ab");
        assert!(screen.editing());
        assert!(screen.error.is_some());
        assert_eq!(screen.reference().keys["left"], "h");
    }

    #[test]
    fn unknown_theme_is_rejected() {
        let mut screen = ConfigScreen::new(Reference::default());
        press(&mut screen, "j");
        type_value(&mut screen, "nope");
        assert!(screen.editing());
        assert_eq!(screen.reference().theme, Reference::default().theme);
    }

    #[test]
    fn unknown_theme_lists_every_theme() {
        let mut screen = ConfigScreen::new(Reference::default());
        press(&mut screen, "j");
        type_value(&mut screen, "nope");
        let shown = draw(40, 24, |f| screen.render(f)).replace('\n', " ");
        for name in palette::names() {
            assert!(shown.contains(name), "{name} missing from\n{shown}");
        }
    }

    #[test]
    fn esc_discards_the_input() {
        let mut screen = ConfigScreen::new(Reference::default());
        press(&mut screen, "jjj");
        screen.handle(KeyCode::Enter);
        press(&mut screen, "q");
        screen.handle(KeyCode::Esc);
        assert!(!screen.editing());
        assert_eq!(screen.reference().keys["down"], "j");
    }

    #[test]
    fn typing_does_not_move_the_cursor() {
        let mut screen = ConfigScreen::new(Reference::default());
        press(&mut screen, "j");
        screen.handle(KeyCode::Enter);
        press(&mut screen, "jjj");
        assert_eq!(screen.cursor, 1);
    }
}
