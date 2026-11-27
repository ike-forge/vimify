use ratatui::Frame;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{ListItem, ListState, Paragraph, Wrap};

use super::cursor_list;
use super::picker::{Picked, Picker};
use crate::reference::Reference;
use crate::{actions, palette};

/// Edits are kept here until `Save changes`.
pub struct ConfigScreen {
    original: Reference,
    reference: Reference,
    /// Rows are the buttons, then the theme, then one row per action.
    cursor: usize,
    input: Option<String>,
    theme: Option<Picker>,
    error: Option<String>,
}

impl ConfigScreen {
    pub fn new(reference: Reference) -> Self {
        Self {
            original: reference.clone(),
            reference,
            cursor: 0,
            input: None,
            theme: None,
            error: None,
        }
    }

    pub fn reference(&self) -> &Reference {
        &self.reference
    }

    /// A text input or the theme picker takes every key, `q` and Esc included.
    pub fn editing(&self) -> bool {
        self.input.is_some() || self.theme.is_some()
    }

    /// Returns true on save.
    pub fn handle(&mut self, key: KeyCode) -> bool {
        if let Some(picker) = &mut self.theme {
            match picker.handle(key) {
                Some(Picked::Choice(theme)) => {
                    self.reference.theme = theme;
                    self.theme = None;
                }
                Some(Picked::Cancel) => self.theme = None,
                None => {}
            }
            return false;
        }
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
        let last = FIRST_FIELD + actions::DEFAULTS.len();
        let on_buttons = self.cursor < FIRST_FIELD;
        match key {
            KeyCode::Char('j') | KeyCode::Down if on_buttons => self.cursor = FIRST_FIELD,
            KeyCode::Char('j') | KeyCode::Down => self.cursor = (self.cursor + 1).min(last),
            KeyCode::Char('k') | KeyCode::Up if self.cursor == FIRST_FIELD => self.cursor = SAVE,
            KeyCode::Char('k') | KeyCode::Up if !on_buttons => self.cursor -= 1,
            KeyCode::Char('h') | KeyCode::Left if on_buttons => self.cursor = SAVE,
            KeyCode::Char('l') | KeyCode::Right if on_buttons => self.cursor = RESET,
            KeyCode::Tab if on_buttons => self.cursor = 1 - self.cursor,
            KeyCode::Char('g') | KeyCode::Home => self.cursor = 0,
            KeyCode::Char('G') | KeyCode::End => self.cursor = last,
            KeyCode::Enter if self.cursor == SAVE => return true,
            KeyCode::Enter if self.cursor == RESET => {
                let defaults = Reference::default();
                self.reference.theme = defaults.theme;
                self.reference.keys = defaults.keys;
                self.error = None;
            }
            KeyCode::Enter if self.cursor == FIRST_FIELD => {
                let themes = palette::names().into_iter().map(str::to_owned).collect();
                self.theme = Some(Picker::new("theme", themes, &self.reference.theme));
                self.error = None;
            }
            KeyCode::Enter => {
                self.input = Some(value(&self.reference, self.cursor - FIRST_FIELD).to_owned());
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
        if actions::is_key(&text) {
            self.error = None;
            let action = actions::DEFAULTS[self.cursor - FIRST_FIELD - 1].0;
            self.reference.keys.insert(action.to_owned(), text);
        } else {
            self.error = Some("One character, or <C-u> style.".to_owned());
            self.input = Some(input);
        }
    }

    pub fn render(&self, frame: &mut Frame) {
        let fields = actions::DEFAULTS.len() + 1;
        let [title, buttons, list, error, footer] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(fields as u16),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .margin(1)
        .areas(frame.area());

        let fields = (0..fields).map(|i| {
            let label = format!("{:10}", name(i)).dim();
            let shown = match &self.input {
                Some(input) if i + FIRST_FIELD == self.cursor => {
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
        let button = |row: usize, label: &'static str| {
            let marker = if self.cursor == row { "› " } else { "  " };
            let label = format!(" {label} ").reversed();
            let label = if self.cursor == row {
                label.bold()
            } else {
                label
            };
            [Span::from(marker).bold(), label]
        };
        let button_row = [
            button(SAVE, "Save changes").to_vec(),
            vec!["  ".into()],
            button(RESET, "Reset to defaults").to_vec(),
        ]
        .concat();
        let mut state = ListState::default().with_selected(self.cursor.checked_sub(FIRST_FIELD));

        frame.render_widget(Line::from(vec!["vimify".bold(), " config".into()]), title);
        frame.render_widget(Line::from(button_row), buttons);
        frame.render_stateful_widget(cursor_list(fields.collect()), list, &mut state);
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
        if let Some(picker) = &self.theme {
            // Over the theme's value: past the cursor symbol and the 10-wide label.
            let anchor = Rect::new(list.x + 12, list.y, 0, 0);
            picker.render(frame, anchor);
        }
    }
}

/// `SAVE` and `RESET` share the top row.
const SAVE: usize = 0;
/// Unsaved like any edit, so `q` still backs out of it.
const RESET: usize = 1;
const FIRST_FIELD: usize = 2;

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
    fn picks_the_theme_by_search() {
        let mut screen = ConfigScreen::new(Reference::default());
        press(&mut screen, "j");
        screen.handle(KeyCode::Enter);
        assert!(screen.editing());
        press(&mut screen, "/sol");
        screen.handle(KeyCode::Enter);
        assert!(!screen.editing());
        assert_eq!(screen.reference().theme, "solarized-dark");
        assert!(draw(80, 20, |f| screen.render(f)).contains("solarized-dark *"));
    }

    #[test]
    fn theme_picker_lists_every_theme() {
        let mut screen = ConfigScreen::new(Reference::default());
        press(&mut screen, "j");
        screen.handle(KeyCode::Enter);
        let shown = draw(80, 24, |f| screen.render(f));
        for name in palette::names() {
            assert!(shown.contains(name), "{name} missing from\n{shown}");
        }
    }

    #[test]
    fn q_closes_the_theme_picker_unchanged() {
        let mut screen = ConfigScreen::new(Reference::default());
        press(&mut screen, "j");
        screen.handle(KeyCode::Enter);
        press(&mut screen, "jq");
        assert!(!screen.editing());
        assert_eq!(screen.reference().theme, Reference::default().theme);
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
    fn buttons_share_a_row() {
        let mut screen = ConfigScreen::new(Reference::default());
        press(&mut screen, "l");
        assert_eq!(screen.cursor, RESET);
        press(&mut screen, "j");
        assert_eq!(screen.cursor, FIRST_FIELD);
        press(&mut screen, "k");
        assert_eq!(screen.cursor, SAVE);
        let shown = draw(80, 20, |f| screen.render(f));
        assert!(
            shown.contains("›  Save changes      Reset to defaults "),
            "{shown}"
        );
    }

    #[test]
    fn reset_restores_defaults_unsaved() {
        let mut edited = Reference {
            theme: "solarized-dark".into(),
            ..Reference::default()
        };
        edited.keys.insert("down".into(), "n".into());
        let mut screen = ConfigScreen::new(edited);
        press(&mut screen, "l");
        assert!(!screen.handle(KeyCode::Enter));
        assert_eq!(screen.reference().theme, Reference::default().theme);
        assert_eq!(screen.reference().keys, Reference::default().keys);
        assert!(draw(80, 20, |f| screen.render(f)).contains("j *"));
    }

    #[test]
    fn typing_does_not_move_the_cursor() {
        let mut screen = ConfigScreen::new(Reference::default());
        press(&mut screen, "jj");
        screen.handle(KeyCode::Enter);
        press(&mut screen, "jj");
        assert_eq!(screen.cursor, FIRST_FIELD + 1);
    }
}
