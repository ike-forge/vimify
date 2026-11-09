use ratatui::Frame;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph, Wrap};

use crate::paths;
use crate::plan::{Action, Op};

pub enum Choice {
    Accept,
    Back,
}

pub struct ConfirmScreen {
    pub ops: Vec<Op>,
    pub exclusions: Vec<String>,
}

impl ConfirmScreen {
    pub fn handle(&self, key: KeyCode) -> Option<Choice> {
        match key {
            KeyCode::Char('y') | KeyCode::Enter => Some(Choice::Accept),
            KeyCode::Char('n') | KeyCode::Esc => Some(Choice::Back),
            _ => None,
        }
    }

    pub fn render(&self, frame: &mut Frame) {
        let [title, warning, list, exclusions, footer] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Fill(1),
            Constraint::Length(2),
            Constraint::Length(1),
        ])
        .margin(1)
        .areas(frame.area());

        let mut warning_text = if self.ops.iter().any(|op| op.action.writes()) {
            vec![Line::from(
                "⚠ Config for these tools will be overridden. Each existing file is backed up next to itself first."
                    .yellow()
                    .bold(),
            )]
        } else {
            vec![Line::from("Nothing to apply.".dim())]
        };
        if self.ops.iter().any(|op| op.action.manual()) {
            warning_text.push(Line::from(
                "⚠ Shortcuts or theme were changed by hand. Confirming overrides those changes; a copy stays in the backup."
                    .red()
                    .bold(),
            ));
        }
        let excluded = if self.exclusions.is_empty() {
            "none".to_owned()
        } else {
            self.exclusions.join(", ")
        };

        frame.render_widget(Line::from(vec!["vimify".bold(), " confirm".into()]), title);
        frame.render_widget(
            Paragraph::new(warning_text).wrap(Wrap { trim: true }),
            warning,
        );
        frame.render_widget(self.list(), list);
        frame.render_widget(
            Line::from(vec!["Excluded on later runs: ".dim(), excluded.into()]),
            exclusions,
        );
        frame.render_widget(Line::from("y confirm  n back  q quit").dim(), footer);
    }

    fn list(&self) -> List<'_> {
        let width = self.ops.iter().map(|op| op.tool.len()).max().unwrap_or(0);
        List::new(self.ops.iter().map(|op| {
            let style = match op.action {
                Action::Apply { .. } => Style::new().blue(),
                Action::Create { .. } => Style::new().green(),
                Action::Update { .. } => Style::new().yellow(),
                Action::Revert { .. } => Style::new().red(),
                Action::Current { .. } | Action::Unsupported => Style::new().dim(),
            };
            let paths = op.action.paths();
            let detail = match paths.first() {
                Some(path) => Span::from(paths::display(path)).dim(),
                None => Span::from("not supported yet").dim(),
            };
            let mut spans = vec![
                Span::styled(format!("{:12}", op.action.label()), style),
                Span::from(format!("{:width$}  ", op.tool)),
                detail,
            ];
            if op.action.manual() {
                let force = format!("  updated manually, force {}", op.action.label());
                spans.push(Span::from(force).red());
            }
            let rest = paths.iter().skip(1).map(|path| {
                let indent = " ".repeat(12 + width + 2);
                Line::from(format!("{indent}{}", paths::display(path)).dim())
            });
            let lines: Vec<Line> = std::iter::once(Line::from(spans)).chain(rest).collect();
            ListItem::new(lines)
        }))
    }
}
