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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Notation {
    Angle,
    Tcell,
    /// gitui: the whole value, `Some(( code: Char('h'), modifiers: ""))`.
    Gitui,
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
            Notation::Vim => Some(key.to_owned()),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

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
