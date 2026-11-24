use ratatui::Frame;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, ListItem, ListState, Paragraph, Wrap};

use super::{cursor_list, row};
use crate::paths;
use crate::plan::Selection;
use crate::port::{self, Port};
use crate::reference::Tools;
use crate::scan::Detection;

/// Rows above the tools: `Apply changes`, `apply everywhere`, `select manually`,
/// `select none`.
const ACTIONS: usize = 4;

struct Tool {
    detection: Detection,
    port: Option<Port>,
    /// Another major version than the port was written for.
    untested: bool,
    checked: bool,
}

impl Tool {
    fn supported(&self) -> bool {
        self.port.as_ref().is_some_and(Port::writable)
    }
}

pub struct SelectScreen {
    tools: Vec<Tool>,
    /// Saves `tools = "all"`; any change to a tool's checkbox leaves it.
    everywhere: bool,
    cursor: usize,
    /// Index into `tools` of the tool shown in the info modal.
    info: Option<usize>,
}

impl SelectScreen {
    /// Supported tools first, so the greyed-out ones collect at the bottom.
    pub fn new(found: Vec<Detection>, ports: &[Port], tools: Option<&Tools>) -> Self {
        let everywhere = matches!(tools, None | Some(Tools::All));
        let listed = |name: &str| tools.is_some_and(|t| t.contains(name));
        let mut tools: Vec<Tool> = found
            .into_iter()
            .map(|detection| {
                let resolved = port::resolve(ports, &detection.port, detection.version.as_ref());
                let (port, untested) = resolved.unzip();
                let mut tool = Tool {
                    detection,
                    port,
                    untested: untested.unwrap_or(false),
                    checked: false,
                };
                tool.checked = tool.supported() && (everywhere || listed(&tool.detection.port));
                tool
            })
            .collect();
        tools.sort_by_key(|t| (!t.supported(), t.detection.port.clone()));
        Self {
            tools,
            everywhere,
            cursor: 0,
            info: None,
        }
    }

    /// The port each tool resolved to.
    pub fn ports(&self) -> Vec<Port> {
        self.tools.iter().filter_map(|t| t.port.clone()).collect()
    }

    /// The modal takes every key, `q` and Esc included.
    pub fn captures_input(&self) -> bool {
        self.info.is_some()
    }

    /// Returns true on confirm.
    pub fn handle(&mut self, key: KeyCode) -> bool {
        if self.info.is_some() {
            self.info = None;
            return false;
        }
        let last = ACTIONS + self.tools.len() - 1;
        match key {
            KeyCode::Enter if self.cursor == 0 => return true,
            KeyCode::Char('j') | KeyCode::Down => self.cursor = (self.cursor + 1).min(last),
            KeyCode::Char('k') | KeyCode::Up => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Char('g') | KeyCode::Home => self.cursor = 0,
            KeyCode::Char('G') | KeyCode::End => self.cursor = last,
            KeyCode::Char(' ') | KeyCode::Enter => self.toggle(),
            KeyCode::Char('i') => self.info = self.cursor.checked_sub(ACTIONS),
            _ => {}
        }
        false
    }

    /// Unsupported tools are in neither list: they are not the user's choice, and with
    /// `tools = "all"` must be applied once a port covers them.
    pub fn selection(&self) -> Selection {
        let (selected, excluded): (Vec<_>, Vec<_>) = self.supported().partition(|t| t.checked);
        let names = |tools: Vec<&Tool>| {
            tools
                .into_iter()
                .map(|t| t.detection.port.clone())
                .collect()
        };
        Selection {
            selected: names(selected),
            excluded: names(excluded),
        }
    }

    fn supported(&self) -> impl Iterator<Item = &Tool> {
        self.tools.iter().filter(|t| t.supported())
    }

    pub fn everywhere(&self) -> bool {
        self.everywhere
    }

    fn toggle(&mut self) {
        match self.cursor {
            0 => {}
            1 => {
                self.everywhere = true;
                self.set_all(true);
            }
            2 => self.everywhere = false,
            3 => {
                self.everywhere = false;
                self.set_all(false);
            }
            i => {
                let tool = &mut self.tools[i - ACTIONS];
                if tool.supported() {
                    tool.checked = !tool.checked;
                    self.everywhere = false;
                }
            }
        }
    }

    fn set_all(&mut self, checked: bool) {
        for tool in &mut self.tools {
            tool.checked = checked && tool.supported();
        }
    }

    fn none_checked(&self) -> bool {
        !self.tools.iter().any(|t| t.checked)
    }

    pub fn render(&self, frame: &mut Frame) {
        let [title, body] = Layout::vertical([Constraint::Length(2), Constraint::Fill(1)])
            .margin(1)
            .areas(frame.area());
        let [list, footer] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(body);
        frame.render_widget(Line::from("space toggle  i details  q quit").dim(), footer);

        let width = self
            .tools
            .iter()
            .map(|t| t.detection.port.len())
            .max()
            .unwrap_or(0);
        let items = [
            ListItem::new(Line::from(" Apply changes ".reversed())),
            radio(
                self.everywhere,
                vec![
                    "apply everywhere".into(),
                    "  every supported tool, also ones installed later".dim(),
                ],
            ),
            radio(!self.everywhere, vec!["select manually".into()]),
            row(self.none_checked(), vec!["select none".into()]),
            ListItem::new(Line::from("─".repeat(width.max("select manually".len()) + 4)).dim()),
        ]
        .into_iter()
        .chain(self.tools.iter().map(|tool| {
            let name = format!("{:width$}  ", tool.detection.port);
            let version = format!("{:9}", version_text(tool));
            if !tool.supported() {
                let text = format!("    {name}{version}not supported yet");
                return ListItem::new(Line::from(text).dim());
            }
            let status = if tool.untested {
                "untested ".yellow()
            } else {
                "supported".green()
            };
            row(
                tool.checked,
                vec![
                    Span::from(name),
                    Span::from(version).dim(),
                    status,
                    Span::from(format!("  {}", paths::display(&tool.detection.path))).dim(),
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

        frame.render_widget(Line::from(vec!["vimify".bold(), " apply".into()]), title);
        frame.render_stateful_widget(cursor_list(items.collect()), list, &mut state);
        if let Some(tool) = self.info.and_then(|i| self.tools.get(i)) {
            let row_width = self
                .tools
                .iter()
                .map(|t| paths::display(&t.detection.path).chars().count())
                .max()
                .unwrap_or(0)
                + width
                + 2 * 3
                + 9
                + 9
                + 2;
            let anchor = Rect {
                x: list.x + row_width as u16 + 2,
                y: list.y + index as u16,
                ..list
            };
            render_info(frame, tool, anchor);
        }
    }
}

fn radio(on: bool, label: Vec<Span<'static>>) -> ListItem<'static> {
    let button = if on { "(•) ".blue() } else { "( ) ".dim() };
    ListItem::new(Line::from([vec![button], label].concat()))
}

fn version_text(tool: &Tool) -> String {
    match &tool.detection.version {
        Some(version) => version.to_string(),
        None => "unknown".to_owned(),
    }
}

/// Placed beside the selected row at `anchor`; centered when it does not fit.
fn render_info(frame: &mut Frame, tool: &Tool, anchor: Rect) {
    let lines = info(tool);
    let screen = frame.area();
    let (width, height) = (60, lines.len() as u16 + 4);
    let fits = anchor.x + width <= screen.right();
    let area = if fits {
        let y = anchor.y.min(screen.bottom().saturating_sub(height));
        Rect::new(anchor.x, y, width, height.min(screen.height))
    } else {
        centered(screen, 76, height)
    };
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .title(format!(" {} ", tool.detection.port))
        .title_bottom(" any key closes ");
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }).block(block),
        area,
    );
}

fn info(tool: &Tool) -> Vec<Line<'static>> {
    let field =
        |name: &str, value: String| Line::from(vec![format!("{name:<11}").bold(), value.into()]);
    let Some(port) = &tool.port else {
        return vec![Line::from("No port for this tool.")];
    };
    let mut lines = Vec::new();
    if let Some(reason) = port.unsupported_reason() {
        lines.push(Line::from(format!("Not supported: {reason}.")));
    }
    lines.push(field("Version", version_text(tool)));
    if tool.untested {
        lines.push(field("Port for", port.versions.to_string()));
    }
    for (path, _) in port.targets.iter().filter_map(|t| t.resolve()) {
        lines.push(field("Config", paths::display(&path)));
    }
    if port.writable() {
        let (bound, unbound) = port.action_coverage();
        let list = |actions: Vec<&str>| {
            if actions.is_empty() {
                "none".to_owned()
            } else {
                actions.join(", ")
            }
        };
        lines.push(field("Keys", list(bound)));
        lines.push(field("Not mapped", list(unbound)));
        lines.push(field("Theme", port.theme_summary()));
    }
    if let Some(notes) = &port.notes {
        lines.push(Line::default());
        lines.push(Line::from(notes.clone()));
    }
    lines
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::port;

    const SUPPORTED: &str = "schema = 1\nversion = \"1\"\nbinaries = []\n[[target]]\nformat = \"yaml\"\nstrategy = \"edit\"\nkey_notation = \"angle\"\npaths = [\"~/.config/c/config.yml\"]\n[target.keys]\na = \"down\"\n[target.theme]\nb = \"{accent}\"\n";
    const UNSUPPORTED: &str = "schema = 1\nversion = \"1\"\nbinaries = []\n";

    fn mixed(supported: &[&str], unsupported: &[&str], tools: Option<Tools>) -> SelectScreen {
        let ports: Vec<Port> = supported
            .iter()
            .map(|n| port::parse(n, SUPPORTED).unwrap())
            .chain(
                unsupported
                    .iter()
                    .map(|n| port::parse(n, UNSUPPORTED).unwrap()),
            )
            .collect();
        let found = ports
            .iter()
            .map(|p| Detection {
                port: p.name.clone(),
                path: format!("/bin/{}", p.name).into(),
                version: Some(semver::Version::new(1, 0, 0)),
            })
            .collect();
        SelectScreen::new(found, &ports, tools.as_ref())
    }

    fn screen(names: &[&str]) -> SelectScreen {
        mixed(names, &[], None)
    }

    fn text(tool: &Tool) -> String {
        info(tool)
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
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
    fn unlisted_tools_start_unchecked() {
        let listed = Tools::Only(vec!["a".into(), "c".into()]);
        let s = mixed(&["a", "b", "c"], &[], Some(listed));
        assert_eq!(s.selection(), selection(&["a", "c"], &["b"]));
    }

    #[test]
    fn first_run_and_all_start_everywhere() {
        assert!(screen(&["a"]).everywhere());
        assert!(mixed(&["a"], &[], Some(Tools::All)).everywhere());
        assert!(!mixed(&["a"], &[], Some(Tools::Only(vec![]))).everywhere());
    }

    #[test]
    fn select_none_then_everywhere() {
        let mut s = screen(&["a", "b"]);
        press(&mut s, "jjj ");
        assert!(s.none_checked() && !s.everywhere());
        press(&mut s, "kk ");
        assert!(s.everywhere());
        assert_eq!(s.selection(), selection(&["a", "b"], &[]));
    }

    #[test]
    fn manual_keeps_the_checks() {
        let mut s = screen(&["a", "b"]);
        press(&mut s, "jj ");
        assert!(!s.everywhere());
        assert_eq!(s.selection(), selection(&["a", "b"], &[]));
    }

    #[test]
    fn toggling_a_tool_updates_action_rows() {
        let mut s = screen(&["a", "b"]);
        press(&mut s, "jjjj ");
        assert_eq!(s.selection(), selection(&["b"], &["a"]));
        assert!(!s.everywhere() && !s.none_checked());
    }

    #[test]
    fn cursor_stays_in_bounds() {
        let mut s = screen(&["a"]);
        press(&mut s, "kkk");
        assert_eq!(s.cursor, 0);
        press(&mut s, "jjjjjj");
        assert_eq!(s.cursor, 4);
        press(&mut s, "g");
        assert_eq!(s.cursor, 0);
        press(&mut s, "G");
        assert_eq!(s.cursor, 4);
    }

    #[test]
    fn unsupported_tools_start_unchecked_and_in_neither_list() {
        let s = mixed(&["a"], &["b"], None);
        assert_eq!(s.selection(), selection(&["a"], &[]));
    }

    #[test]
    fn unsupported_tools_cannot_be_toggled() {
        let mut s = mixed(&[], &["a"], None);
        press(&mut s, "jjjj ");
        assert_eq!(s.selection(), selection(&[], &[]));
        assert!(!s.tools[0].checked);
        assert!(s.everywhere());
    }

    #[test]
    fn everywhere_skips_unsupported_tools() {
        let mut s = mixed(&["a"], &["b"], None);
        press(&mut s, "jjj ");
        assert!(s.none_checked());
        press(&mut s, "kk ");
        assert_eq!(s.selection(), selection(&["a"], &[]));
        assert!(!s.tools[1].checked);
    }

    #[test]
    fn supported_tools_sort_first() {
        let s = mixed(&["z"], &["a"], None);
        let names: Vec<_> = s.tools.iter().map(|t| t.detection.port.as_str()).collect();
        assert_eq!(names, ["z", "a"]);
    }

    #[test]
    fn info_opens_on_a_tool_and_any_key_closes_it() {
        let mut s = screen(&["a"]);
        press(&mut s, "i");
        assert!(!s.captures_input());
        press(&mut s, "jjjji");
        assert!(s.captures_input());

        assert!(!s.handle(KeyCode::Enter));

        assert!(!s.captures_input());
    }

    #[test]
    fn keys_do_not_reach_the_list_while_the_modal_is_open() {
        let mut s = screen(&["a"]);
        press(&mut s, "jjjji ");
        assert!(s.tools[0].checked);
    }

    #[test]
    fn info_lists_what_the_port_covers() {
        let s = screen(&["a"]);
        let out = text(&s.tools[0]);
        assert!(out.contains("c/config.yml"), "{out}");
        assert!(out.contains("Keys       down"), "{out}");
        assert!(
            out.contains("Not mapped left, up, right, yank, delete, quit"),
            "{out}"
        );
        assert!(out.contains("Theme      1 settings"), "{out}");
    }

    #[test]
    fn info_says_why_a_tool_is_unsupported() {
        let s = mixed(&[], &["a"], None);
        let out = text(&s.tools[0]);
        assert!(out.contains("Not supported:"), "{out}");
        assert!(!out.contains("Keys"), "{out}");
    }

    #[test]
    fn untested_is_shown_in_the_info_modal() {
        let mut port = port::parse("a", SUPPORTED).unwrap();
        port.versions = ">=1.0, <2.0".parse().unwrap();
        let found = vec![Detection {
            port: "a".into(),
            path: "/bin/a".into(),
            version: Some(semver::Version::new(2, 1, 0)),
        }];

        let s = SelectScreen::new(found, &[port], None);

        assert!(s.tools[0].untested);
        assert!(s.tools[0].checked);
        let out = text(&s.tools[0]);
        assert!(out.contains("Version    2.1.0"), "{out}");
        assert!(out.contains("Port for   >=1.0, <2.0"), "{out}");
    }

    #[test]
    fn enter_confirms_on_the_button_and_toggles_elsewhere() {
        let mut s = screen(&["a"]);
        press(&mut s, "G");
        assert!(!s.handle(KeyCode::Enter));
        assert!(!s.tools[0].checked);
        press(&mut s, "g");
        assert!(s.handle(KeyCode::Enter));
    }
}
