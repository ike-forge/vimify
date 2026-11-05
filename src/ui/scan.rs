use std::path::PathBuf;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Gauge, List, ListItem};

use crate::paths;
use crate::scan::{Detection, Event};

pub struct ScanScreen {
    total: usize,
    done: usize,
    current: Option<PathBuf>,
    found: Vec<Detection>,
    finished: bool,
}

impl ScanScreen {
    pub fn new(total: usize) -> Self {
        Self {
            total,
            done: 0,
            current: None,
            found: Vec::new(),
            finished: false,
        }
    }

    pub fn update(&mut self, event: Event) {
        match event {
            Event::Scanned { done, dir } => {
                self.done = done;
                self.current = Some(dir);
            }
            Event::Found(d) => self.found.push(d),
            Event::Version { port, version } => {
                if let Some(d) = self.found.iter_mut().find(|d| d.port == port) {
                    d.version = Some(version);
                }
            }
            Event::Done => self.finished = true,
        }
    }

    pub fn finished(&self) -> bool {
        self.finished
    }

    pub fn found(&self) -> &[Detection] {
        &self.found
    }

    pub fn render(&self, frame: &mut Frame) {
        let [title, gauge, list, footer] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .margin(1)
        .areas(frame.area());

        frame.render_widget(Line::from(vec!["vimify".bold(), " scan".into()]), title);
        frame.render_widget(self.gauge(), gauge);
        frame.render_widget(self.list(), list);
        frame.render_widget(Line::from("q quit").dim(), footer);
    }

    fn gauge(&self) -> Gauge<'_> {
        let ratio = if self.total == 0 {
            1.0
        } else {
            self.done as f64 / self.total as f64
        };
        let label = if self.finished {
            format!("{} known tools found", self.found.len())
        } else {
            let dir = self
                .current
                .as_deref()
                .map(paths::display)
                .unwrap_or_default();
            format!("{}/{}  {dir}", self.done, self.total)
        };
        Gauge::default()
            .ratio(ratio)
            .label(label)
            .gauge_style(Style::new().blue())
    }

    fn list(&self) -> List<'_> {
        let width = self.found.iter().map(|d| d.port.len()).max().unwrap_or(0);
        List::new(self.found.iter().map(|d| {
            ListItem::new(Line::from(vec![
                Span::from(format!("{:width$}  ", d.port)),
                Span::from(paths::display(&d.path)).dim(),
            ]))
        }))
    }
}
