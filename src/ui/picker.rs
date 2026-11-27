use ratatui::Frame;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, ListItem, ListState};

use super::cursor_list;

/// A modal list of fixed choices, so no invalid value can be picked.
pub struct Picker {
    title: &'static str,
    choices: Vec<String>,
    current: String,
    query: String,
    searching: bool,
    /// Index into `matches()`.
    cursor: usize,
}

pub enum Picked {
    Choice(String),
    Cancel,
}

impl Picker {
    pub fn new(title: &'static str, choices: Vec<String>, current: &str) -> Self {
        let cursor = choices.iter().position(|c| c == current).unwrap_or(0);
        Self {
            title,
            choices,
            current: current.to_owned(),
            query: String::new(),
            searching: false,
            cursor,
        }
    }

    /// Case-insensitive substring match on the query.
    fn matches(&self) -> Vec<&str> {
        let query = self.query.to_lowercase();
        self.choices
            .iter()
            .filter(|c| c.to_lowercase().contains(&query))
            .map(String::as_str)
            .collect()
    }

    pub fn handle(&mut self, key: KeyCode) -> Option<Picked> {
        let count = self.matches().len();
        let last = count.saturating_sub(1);
        if self.searching {
            match key {
                KeyCode::Char(c) => self.set_query(format!("{}{c}", self.query)),
                KeyCode::Backspace => {
                    let mut query = self.query.clone();
                    query.pop();
                    self.set_query(query);
                }
                KeyCode::Down => self.cursor = (self.cursor + 1).min(last),
                KeyCode::Up => self.cursor = self.cursor.saturating_sub(1),
                KeyCode::Enter => return self.pick(),
                KeyCode::Esc => {
                    self.searching = false;
                    self.set_query(String::new());
                }
                _ => {}
            }
            return None;
        }
        match key {
            KeyCode::Char('j') | KeyCode::Down => self.cursor = (self.cursor + 1).min(last),
            KeyCode::Char('k') | KeyCode::Up => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Char('g') | KeyCode::Home => self.cursor = 0,
            KeyCode::Char('G') | KeyCode::End => self.cursor = last,
            KeyCode::Char('/') => self.searching = true,
            KeyCode::Enter => return self.pick(),
            KeyCode::Char('q') | KeyCode::Esc => return Some(Picked::Cancel),
            _ => {}
        }
        None
    }

    fn set_query(&mut self, query: String) {
        self.query = query;
        self.cursor = 0;
    }

    /// Nothing to pick when the filter matches nothing.
    fn pick(&self) -> Option<Picked> {
        let choice = self.matches().get(self.cursor)?.to_string();
        Some(Picked::Choice(choice))
    }

    /// Placed at `anchor`, moved up when it would run off the bottom.
    pub fn render(&self, frame: &mut Frame, anchor: Rect) {
        let screen = frame.area();
        let width = self.choices.iter().map(|c| c.len()).max().unwrap_or(0) as u16 + 8;
        let width = width.max(24).min(screen.width);
        let height = (self.choices.len() as u16 + 3).min(screen.height);
        let x = anchor.x.min(screen.right().saturating_sub(width));
        let y = anchor.y.min(screen.bottom().saturating_sub(height));
        let area = Rect::new(x, y, width, height);

        let hint = if self.searching {
            " enter pick  esc clear "
        } else {
            " / search  esc close "
        };
        let block = Block::bordered()
            .title(format!(" {} ", self.title))
            .title_bottom(hint);
        let inner = block.inner(area);
        frame.render_widget(Clear, area);
        frame.render_widget(block, area);
        let [search, list] =
            Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(inner);

        if self.searching {
            let line = Line::from(vec![format!("/{}", self.query).into(), " ".reversed()]);
            frame.render_widget(line, search);
        }

        let matches = self.matches();
        if matches.is_empty() {
            frame.render_widget(Line::from("  no match").dim(), list);
            return;
        }
        let items = matches
            .iter()
            .map(|c| {
                if *c == self.current {
                    ListItem::new(Line::from(format!("{c} •")))
                } else {
                    ListItem::new(Line::from(c.to_string()))
                }
            })
            .collect();
        let mut state = ListState::default().with_selected(Some(self.cursor));
        frame.render_stateful_widget(cursor_list(items), list, &mut state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::tests::draw;

    fn picker() -> Picker {
        let choices = ["catppuccin-frappe", "catppuccin-mocha", "solarized-dark"];
        Picker::new(
            "theme",
            choices.map(str::to_owned).to_vec(),
            "catppuccin-mocha",
        )
    }

    fn press(p: &mut Picker, keys: &str) -> Option<Picked> {
        keys.chars().map(|c| p.handle(KeyCode::Char(c))).last()?
    }

    fn picked(result: Option<Picked>) -> String {
        match result {
            Some(Picked::Choice(c)) => c,
            _ => panic!("nothing picked"),
        }
    }

    #[test]
    fn starts_on_the_current_choice() {
        let mut p = picker();
        assert_eq!(picked(p.handle(KeyCode::Enter)), "catppuccin-mocha");
    }

    #[test]
    fn moves_and_picks() {
        let mut p = picker();
        press(&mut p, "jjj");
        assert_eq!(picked(p.handle(KeyCode::Enter)), "solarized-dark");
    }

    #[test]
    fn search_filters_and_enter_picks_the_match() {
        let mut p = picker();
        press(&mut p, "/SOL");
        assert_eq!(picked(p.handle(KeyCode::Enter)), "solarized-dark");
    }

    #[test]
    fn typed_q_and_j_go_into_the_query() {
        let mut p = picker();
        assert!(press(&mut p, "/qj").is_none());
        assert_eq!(p.query, "qj");
    }

    #[test]
    fn no_match_picks_nothing() {
        let mut p = picker();
        press(&mut p, "/nope");
        assert!(p.handle(KeyCode::Enter).is_none());
        assert!(draw(40, 10, |f| p.render(f, f.area())).contains("no match"));
    }

    #[test]
    fn esc_ends_the_search_before_closing() {
        let mut p = picker();
        press(&mut p, "/sol");
        p.handle(KeyCode::Esc);
        assert_eq!(p.matches().len(), 3);
        assert!(matches!(p.handle(KeyCode::Esc), Some(Picked::Cancel)));
    }

    #[test]
    fn q_closes() {
        let mut p = picker();
        assert!(matches!(press(&mut p, "q"), Some(Picked::Cancel)));
    }

    #[test]
    fn marks_the_current_choice() {
        let p = picker();
        assert!(draw(40, 10, |f| p.render(f, f.area())).contains("catppuccin-mocha •"));
    }
}
