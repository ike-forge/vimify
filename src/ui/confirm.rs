use ratatui::Frame;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph, Wrap};

use crate::paths;
use crate::plan::{Action, Op};
use crate::reference::Tools;

pub enum Choice {
    Accept,
    Back,
}

pub struct ConfirmScreen {
    pub ops: Vec<Op>,
    pub tools: Tools,
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
        let legend_lines = self.legend();
        let [title, warning, list, legend, tools, footer] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Fill(1),
            Constraint::Length(legend_lines.len() as u16 + 1),
            Constraint::Length(2),
            Constraint::Length(1),
        ])
        .margin(1)
        .areas(frame.area());

        let mut warning_text = if self.ops.iter().any(|op| op.action.writes()) {
            vec![Line::from(
                "⚠️ Config for these tools will be overridden. Each existing file is backed up next to itself first."
                    .yellow()
                    .bold(),
            )]
        } else {
            vec![Line::from("Nothing to apply.".dim())]
        };
        if self.ops.iter().any(|op| op.action.manual()) {
            warning_text.push(Line::from(
                "⚠️ Shortcuts or theme were changed by hand. Confirming overrides those changes; a copy stays in the backup."
                    .red()
                    .bold(),
            ));
        }
        let managed = match &self.tools {
            Tools::All => "all".to_owned(),
            Tools::Only(list) if list.is_empty() => "none".to_owned(),
            Tools::Only(list) => list.join(", "),
        };

        frame.render_widget(Line::from(vec!["vimify".bold(), " confirm".into()]), title);
        frame.render_widget(
            Paragraph::new(warning_text).wrap(Wrap { trim: true }),
            warning,
        );
        frame.render_widget(self.list(), list);
        frame.render_widget(Paragraph::new(legend_lines), legend);
        frame.render_widget(
            Line::from(vec!["Managed on later runs: ".dim(), managed.into()]),
            tools,
        );
        frame.render_widget(Line::from("y confirm  n back  q quit").dim(), footer);
    }

    /// One line per action present, in first-seen order.
    fn legend(&self) -> Vec<Line<'_>> {
        let mut seen = Vec::new();
        for op in &self.ops {
            let label = op.action.label();
            if !seen.iter().any(|(l, _)| *l == label) {
                seen.push((label, &op.action));
            }
        }
        seen.into_iter()
            .map(|(label, action)| {
                Line::from(vec![
                    Span::styled(format!("{label:12}"), style(action)),
                    Span::from(meaning(action)).dim(),
                ])
            })
            .collect()
    }

    fn list(&self) -> List<'_> {
        let width = self.ops.iter().map(|op| op.tool.len()).max().unwrap_or(0);
        List::new(self.ops.iter().map(|op| {
            let style = style(&op.action);
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

fn style(action: &Action) -> Style {
    match action {
        Action::Apply { .. } => Style::new().blue(),
        Action::Create { .. } => Style::new().green(),
        Action::Update { .. } => Style::new().yellow(),
        Action::Revert { .. } => Style::new().red(),
        Action::Current { .. } | Action::Unsupported => Style::new().dim(),
    }
}

fn meaning(action: &Action) -> &'static str {
    match action {
        Action::Apply { .. } => "first run for this tool; merges into its existing config",
        Action::Create { .. } => "first run for this tool; no config yet, writes a new one",
        Action::Update { .. } => {
            "applied before; port, reference or a hand-edited key changed since"
        }
        Action::Revert { .. } => "deselected; restores the config from before vimify",
        Action::Current { .. } => "applied before and unchanged; skipped",
        Action::Unsupported => "port has no editor or mappings yet; skipped",
    }
}
