use serde::Deserialize;

/// Every action vimify knows, with its default key. Ports cannot add to this.
pub const DEFAULTS: &[(&str, &str)] = &[
    ("left", "h"),
    ("down", "j"),
    ("up", "k"),
    ("right", "l"),
    ("yank", "y"),
    ("delete", "d"),
    ("quit", "q"),
];

pub fn is_action(name: &str) -> bool {
    DEFAULTS.iter().any(|(action, _)| *action == name)
}

/// Shape check only; whether a tool can spell the key is up to each port's notation.
pub fn is_key(key: &str) -> bool {
    if key.chars().any(char::is_whitespace) {
        return false;
    }
    key.chars().count() == 1 || (key.len() > 2 && key.starts_with('<') && key.ends_with('>'))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Notation {
    Angle,
    Tcell,
    /// gitui: the whole value, `Some(( code: Char('h'), modifiers: ""))`.
    Gitui,
    /// stu: `ctrl-u`, `shift-g`, inside a list.
    Stu,
    /// less's lesskey file: `^U`, `\e` for alt, `\kd` for arrows.
    Lesskey,
    /// The tool spells keys as the reference does.
    Vim,
}

impl Notation {
    /// `None` if the tool cannot spell `key`; the binding is then skipped with a warning.
    pub fn convert(self, key: &str) -> Option<String> {
        match self {
            Notation::Angle => angle(key),
            Notation::Tcell => tcell(key),
            Notation::Gitui => gitui(key),
            Notation::Stu => stu(key),
            Notation::Lesskey => lesskey(key),
            Notation::Vim => Some(key.to_owned()),
        }
    }

    /// The tool takes a list of keys per binding, so the key is written as one.
    pub fn is_list(self) -> bool {
        self == Notation::Stu
    }
}

fn angle(key: &str) -> Option<String> {
    if key.chars().count() == 1 {
        return Some(key.to_owned());
    }
    let inner = key.strip_prefix('<')?.strip_suffix('>')?.to_lowercase();
    if let Some(("c" | "a", ch)) = inner.split_once('-')
        && ch.chars().count() == 1
    {
        return Some(format!("<{inner}>"));
    }
    let name = match inner.as_str() {
        "cr" | "enter" | "return" => "enter",
        "bs" | "backspace" => "backspace",
        "del" | "delete" => "delete",
        "pageup" | "pgup" => "pgup",
        "pagedown" | "pgdown" | "pgdn" => "pgdown",
        "esc" | "tab" | "space" | "up" | "down" | "left" | "right" | "home" | "end" | "insert" => {
            &inner
        }
        _ => return None,
    };
    Some(format!("<{name}>"))
}

fn gitui(key: &str) -> Option<String> {
    let (code, modifiers) = if key.chars().count() == 1 {
        let ch = key.chars().next()?;
        (
            char_code(ch),
            if ch.is_ascii_uppercase() { "SHIFT" } else { "" },
        )
    } else {
        let inner = key.strip_prefix('<')?.strip_suffix('>')?.to_lowercase();
        if let Some((m @ ("c" | "a"), ch)) = inner.split_once('-')
            && ch.chars().count() == 1
        {
            let modifiers = if m == "c" { "CONTROL" } else { "ALT" };
            (char_code(ch.chars().next()?), modifiers)
        } else {
            let name = match inner.as_str() {
                "cr" | "enter" | "return" => "Enter",
                "bs" | "backspace" => "Backspace",
                "del" | "delete" => "Delete",
                "pageup" | "pgup" => "PageUp",
                "pagedown" | "pgdown" | "pgdn" => "PageDown",
                "space" => return Some(gitui_value(&char_code(' '), "")),
                "esc" => "Esc",
                "tab" => "Tab",
                "up" => "Up",
                "down" => "Down",
                "left" => "Left",
                "right" => "Right",
                "home" => "Home",
                "end" => "End",
                "insert" => "Insert",
                _ => return None,
            };
            (name.to_owned(), "")
        }
    };
    Some(gitui_value(&code, modifiers))
}

fn char_code(ch: char) -> String {
    match ch {
        '\'' => "Char('\\'')".to_owned(),
        '\\' => "Char('\\\\')".to_owned(),
        _ => format!("Char('{ch}')"),
    }
}

fn gitui_value(code: &str, modifiers: &str) -> String {
    format!("Some(( code: {code}, modifiers: \"{modifiers}\"))")
}

/// A name tcell lacks stops lazysql from starting, so this only emits names from
/// tcell's `KeyNames`.
fn tcell(key: &str) -> Option<String> {
    if key.chars().count() == 1 {
        return Some(key.to_owned());
    }
    let inner = key.strip_prefix('<')?.strip_suffix('>')?.to_lowercase();
    if let Some(("c", ch)) = inner.split_once('-') {
        let mut chars = ch.chars();
        let (Some(ch), None) = (chars.next(), chars.next()) else {
            return None;
        };
        return match ch {
            // These bytes are named after the keys that send them.
            'h' => Some("Backspace".to_owned()),
            'i' => Some("Tab".to_owned()),
            'm' => Some("Enter".to_owned()),
            'a'..='z' => Some(format!("Ctrl-{}", ch.to_ascii_uppercase())),
            _ => None,
        };
    }
    let name = match inner.as_str() {
        "cr" | "enter" | "return" => "Enter",
        "bs" | "backspace" => "Backspace2",
        "del" | "delete" => "Delete",
        "esc" => "Esc",
        "tab" => "Tab",
        "space" => " ",
        "up" => "Up",
        "down" => "Down",
        "left" => "Left",
        "right" => "Right",
        "home" => "Home",
        "end" => "End",
        "insert" => "Insert",
        "pageup" | "pgup" => "PgUp",
        "pagedown" | "pgdown" | "pgdn" => "PgDn",
        _ => return None,
    };
    Some(name.to_owned())
}

fn lesskey(key: &str) -> Option<String> {
    let literal = |ch: char| match ch {
        // A line starting with # is a comment; ^ and \ are lesskey's own escapes.
        '#' | '^' | '\\' => Some(format!("\\{ch}")),
        ch if ch.is_ascii_graphic() => Some(ch.to_string()),
        _ => None,
    };
    if key.chars().count() == 1 {
        return literal(key.chars().next()?);
    }
    let inner = key.strip_prefix('<')?.strip_suffix('>')?.to_lowercase();
    if let Some((m @ ("c" | "a"), ch)) = inner.split_once('-') {
        let mut chars = ch.chars();
        let (Some(ch), None) = (chars.next(), chars.next()) else {
            return None;
        };
        return match m {
            "c" if ch.is_ascii_lowercase() => Some(format!("^{}", ch.to_ascii_uppercase())),
            "a" => Some(format!("\\e{}", literal(ch)?)),
            _ => None,
        };
    }
    let name = match inner.as_str() {
        "cr" | "enter" | "return" => "\\r",
        "bs" | "backspace" => "\\b",
        "del" | "delete" => "\\kx",
        "esc" => "\\e",
        "tab" => "\\t",
        "space" => "\\ ",
        "up" => "\\ku",
        "down" => "\\kd",
        "left" => "\\kl",
        "right" => "\\kr",
        "home" => "\\kh",
        "end" => "\\ke",
        "pageup" | "pgup" => "\\kU",
        "pagedown" | "pgdown" | "pgdn" => "\\kD",
        _ => return None,
    };
    Some(name.to_owned())
}

/// stu lowercases what it reads, so `G` must be `shift-g`, and it reads one byte as a
/// char, so non-ASCII keys cannot be bound.
fn stu(key: &str) -> Option<String> {
    if key.chars().count() == 1 {
        return match key.chars().next()? {
            ' ' => Some("space".to_owned()),
            ch if ch.is_ascii_uppercase() => Some(format!("shift-{}", ch.to_ascii_lowercase())),
            ch if ch.is_ascii_graphic() => Some(key.to_owned()),
            _ => None,
        };
    }
    let inner = key.strip_prefix('<')?.strip_suffix('>')?.to_lowercase();
    if let Some((m @ ("c" | "a"), ch)) = inner.split_once('-')
        && ch.chars().count() == 1
        && ch.chars().all(|c| c.is_ascii_graphic())
    {
        let modifier = if m == "c" { "ctrl" } else { "alt" };
        return Some(format!("{modifier}-{ch}"));
    }
    let name = match inner.as_str() {
        "cr" | "enter" | "return" => "enter",
        "bs" | "backspace" => "backspace",
        "del" | "delete" => "delete",
        "pageup" | "pgup" => "pageup",
        "pagedown" | "pgdown" | "pgdn" => "pagedown",
        "esc" | "tab" | "space" | "up" | "down" | "left" | "right" | "home" | "end" | "insert" => {
            &inner
        }
        _ => return None,
    };
    Some(name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stu(key: &str) -> Option<String> {
        Notation::Stu.convert(key)
    }

    #[test]
    fn lesskey_escapes_its_own_syntax() {
        let lesskey = |k| Notation::Lesskey.convert(k);
        assert_eq!(lesskey("j").as_deref(), Some("j"));
        assert_eq!(lesskey("#").as_deref(), Some("\\#"));
        assert_eq!(lesskey("^").as_deref(), Some("\\^"));
        assert_eq!(lesskey("\\").as_deref(), Some("\\\\"));
    }

    #[test]
    fn lesskey_spells_modifiers_and_names() {
        let lesskey = |k| Notation::Lesskey.convert(k);
        assert_eq!(lesskey("<C-u>").as_deref(), Some("^U"));
        assert_eq!(lesskey("<A-j>").as_deref(), Some("\\ej"));
        assert_eq!(lesskey("<Space>").as_deref(), Some("\\ "));
        assert_eq!(lesskey("<Down>").as_deref(), Some("\\kd"));
        assert_eq!(lesskey("<PageUp>").as_deref(), Some("\\kU"));
        assert_eq!(lesskey("<F1>"), None);
        assert_eq!(lesskey("é"), None);
    }

    #[test]
    fn stu_spells_uppercase_with_shift() {
        assert_eq!(stu("j").as_deref(), Some("j"));
        assert_eq!(stu("G").as_deref(), Some("shift-g"));
        assert_eq!(stu("?").as_deref(), Some("?"));
        assert_eq!(stu(" ").as_deref(), Some("space"));
    }

    #[test]
    fn stu_spells_modifiers_and_names() {
        assert_eq!(stu("<C-u>").as_deref(), Some("ctrl-u"));
        assert_eq!(stu("<A-x>").as_deref(), Some("alt-x"));
        assert_eq!(stu("<CR>").as_deref(), Some("enter"));
        assert_eq!(stu("<PageDown>").as_deref(), Some("pagedown"));
    }

    #[test]
    fn stu_refuses_what_it_cannot_parse() {
        assert_eq!(stu("ä"), None);
        assert_eq!(stu("gg"), None);
        assert_eq!(stu("<F5>"), None);
    }

    fn angle(key: &str) -> Option<String> {
        Notation::Angle.convert(key)
    }

    fn tcell(key: &str) -> Option<String> {
        Notation::Tcell.convert(key)
    }

    #[test]
    fn gitui_spells_keys_as_crossterm_events() {
        let gitui = |k| Notation::Gitui.convert(k);
        assert_eq!(
            gitui("h").as_deref(),
            Some("Some(( code: Char('h'), modifiers: \"\"))")
        );
        assert_eq!(
            gitui("G").as_deref(),
            Some("Some(( code: Char('G'), modifiers: \"SHIFT\"))")
        );
        assert_eq!(
            gitui("<C-u>").as_deref(),
            Some("Some(( code: Char('u'), modifiers: \"CONTROL\"))")
        );
        assert_eq!(
            gitui("<PageDown>").as_deref(),
            Some("Some(( code: PageDown, modifiers: \"\"))")
        );
        assert_eq!(
            gitui("'").as_deref(),
            Some("Some(( code: Char('\\''), modifiers: \"\"))")
        );
        assert_eq!(gitui("<F5>"), None);
    }

    #[test]
    fn tcell_keeps_single_chars() {
        assert_eq!(tcell("j").as_deref(), Some("j"));
        assert_eq!(tcell("G").as_deref(), Some("G"));
    }

    #[test]
    fn tcell_uppercases_ctrl() {
        assert_eq!(tcell("<C-u>").as_deref(), Some("Ctrl-U"));
    }

    #[test]
    fn tcell_has_no_ctrl_h_i_or_m() {
        assert_eq!(tcell("<C-h>").as_deref(), Some("Backspace"));
        assert_eq!(tcell("<C-i>").as_deref(), Some("Tab"));
        assert_eq!(tcell("<C-m>").as_deref(), Some("Enter"));
    }

    #[test]
    fn tcell_maps_vim_names() {
        assert_eq!(tcell("<CR>").as_deref(), Some("Enter"));
        assert_eq!(tcell("<PageDown>").as_deref(), Some("PgDn"));
        assert_eq!(tcell("<Space>").as_deref(), Some(" "));
    }

    #[test]
    fn tcell_refuses_what_it_cannot_spell() {
        assert_eq!(tcell("<A-x>"), None);
        assert_eq!(tcell("gg"), None);
        assert_eq!(tcell("<C-1>"), None);
    }

    #[test]
    fn single_char_is_unchanged() {
        assert_eq!(angle("j").as_deref(), Some("j"));
        assert_eq!(angle("G").as_deref(), Some("G"));
    }

    #[test]
    fn modifiers_are_lowercased() {
        assert_eq!(angle("<C-u>").as_deref(), Some("<c-u>"));
        assert_eq!(angle("<A-x>").as_deref(), Some("<a-x>"));
    }

    #[test]
    fn vim_names_map_to_tool_names() {
        assert_eq!(angle("<CR>").as_deref(), Some("<enter>"));
        assert_eq!(angle("<BS>").as_deref(), Some("<backspace>"));
        assert_eq!(angle("<PageDown>").as_deref(), Some("<pgdown>"));
        assert_eq!(angle("<Esc>").as_deref(), Some("<esc>"));
    }

    #[test]
    fn inexpressible_keys_are_none() {
        assert_eq!(angle("gg"), None);
        assert_eq!(angle("<leader>x"), None);
        assert_eq!(angle("<F13>"), None);
    }
}
