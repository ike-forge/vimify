use serde::Deserialize;

/// Every action vimify knows, with its default key. Ports cannot add to this.
pub const DEFAULTS: &[(&str, &str)] = &[
    ("left", "h"),
    ("down", "j"),
    ("up", "k"),
    ("right", "l"),
    ("yank", "y"),
    ("delete", "d"),
];

pub fn is_action(name: &str) -> bool {
    DEFAULTS.iter().any(|(action, _)| *action == name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Notation {
    Angle,
}

impl Notation {
    /// `None` if the tool cannot spell `key`; the binding is then skipped with a warning.
    pub fn convert(self, key: &str) -> Option<String> {
        match self {
            Notation::Angle => angle(key),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn angle(key: &str) -> Option<String> {
        Notation::Angle.convert(key)
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
