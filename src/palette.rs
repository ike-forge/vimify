use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use include_dir::{Dir, include_dir};

static EMBEDDED: Dir = include_dir!("$CARGO_MANIFEST_DIR/palettes");

pub const ROLES: &[&str] = &[
    "accent",
    "selection",
    "text",
    "muted",
    "border",
    "success",
    "warning",
    "error",
    "special",
    "info",
    "base",
    "surface",
];

#[derive(Debug, Clone)]
pub struct Palette {
    roles: BTreeMap<String, String>,
    /// Named colours the user can pick as `accent`, with the palette's own pick.
    accents: BTreeMap<String, String>,
    accent: Option<String>,
}

impl Palette {
    pub fn role(&self, name: &str) -> Option<&str> {
        self.roles.get(name).map(String::as_str)
    }

    /// Name of the accent in use, for upstream themes that ship one file per accent.
    pub fn accent_name(&self) -> Option<&str> {
        self.accent.as_deref()
    }

    /// `None` keeps the palette's accent.
    pub fn with_accent(mut self, accent: Option<&str>) -> Result<Self> {
        let Some(name) = accent else {
            return Ok(self);
        };
        let Some(color) = self.accents.get(name) else {
            let known: Vec<&str> = self.accents.keys().map(String::as_str).collect();
            bail!("unknown accent {name}. Known: {}", known.join(", "));
        };
        self.roles.insert("accent".to_owned(), color.clone());
        self.accent = Some(name.to_owned());
        Ok(self)
    }
}

#[derive(serde::Deserialize)]
struct PaletteFile {
    #[serde(default)]
    accents: BTreeMap<String, String>,
    #[serde(flatten)]
    roles: BTreeMap<String, String>,
}

/// Roles a port value refers to: `"{accent}"` has `accent`, `bold` has none.
pub fn placeholders(text: &str) -> impl Iterator<Item = &str> {
    text.split('{')
        .skip(1)
        .filter_map(|rest| rest.split_once('}'))
        .map(|(role, _)| role)
}

pub fn names() -> Vec<&'static str> {
    EMBEDDED
        .files()
        .filter_map(|f| f.path().to_str()?.strip_suffix(".toml"))
        .collect()
}

pub fn load(theme: &str) -> Result<Palette> {
    let text = EMBEDDED
        .get_file(format!("{theme}.toml"))
        .and_then(|f| f.contents_utf8())
        .with_context(|| format!("unknown theme {theme}"))?;
    parse(text).with_context(|| format!("theme {theme}"))
}

/// `accent` may name one of `[accents]` instead of being a colour.
fn parse(text: &str) -> Result<Palette> {
    let PaletteFile { accents, mut roles } = toml::from_str(text)?;
    for (name, color) in &accents {
        if !is_hex(color) {
            bail!("accent {name}: {color} is not a #rrggbb colour");
        }
    }
    let accent = match roles.get("accent") {
        Some(name) if accents.contains_key(name) => Some(name.clone()),
        _ => None,
    };
    if let Some(name) = &accent {
        roles.insert("accent".to_owned(), accents[name].clone());
    }
    for role in ROLES {
        if !roles.contains_key(*role) {
            bail!("missing role {role}");
        }
    }
    for (role, color) in &roles {
        if !ROLES.contains(&role.as_str()) {
            bail!("unknown role {role}");
        }
        if !is_hex(color) {
            bail!("{role}: {color} is not a #rrggbb colour");
        }
    }
    Ok(Palette {
        roles,
        accents,
        accent,
    })
}

fn is_hex(color: &str) -> bool {
    color
        .strip_prefix('#')
        .is_some_and(|hex| hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_palettes_parse() {
        let palette = load("catppuccin-frappe").unwrap();
        assert_eq!(palette.role("accent"), Some("#8caaee"));
        assert_eq!(names().len(), 6);
        for name in names() {
            load(name).unwrap();
        }
    }

    #[test]
    fn an_accent_name_picks_from_the_accents() {
        let palette = load("catppuccin-frappe").unwrap();
        assert_eq!(palette.accent_name(), Some("blue"));
        let mauve = palette.with_accent(Some("mauve")).unwrap();
        assert_eq!(mauve.role("accent"), Some("#ca9ee6"));
        assert_eq!(mauve.accent_name(), Some("mauve"));
    }

    #[test]
    fn a_palette_without_accents_refuses_one() {
        let palette = load("solarized-dark").unwrap();
        assert_eq!(palette.accent_name(), None);
        let err = palette.with_accent(Some("blue")).unwrap_err();
        assert!(err.to_string().contains("unknown accent blue"));
    }

    #[test]
    fn unknown_theme_is_an_error() {
        assert!(load("nope").unwrap_err().to_string().contains("nope"));
    }

    const FULL: &str = include_str!("../palettes/catppuccin-frappe.toml");

    #[test]
    fn missing_role_is_an_error() {
        let err = parse(&FULL.replace("selection = \"#414559\"\n", "")).unwrap_err();
        assert!(err.to_string().contains("selection"));
    }

    #[test]
    fn unknown_role_is_an_error() {
        let err = parse(&format!("new = \"#000000\"\n{FULL}")).unwrap_err();
        assert!(err.to_string().contains("new"));
    }

    #[test]
    fn invalid_colour_is_an_error() {
        let err = parse(&FULL.replace("\"blue\"\n", "\"navy\"\n")).unwrap_err();
        assert!(err.to_string().contains("navy"));
    }

    #[test]
    fn placeholders_can_sit_inside_text() {
        let roles = |text| placeholders(text).collect::<Vec<_>>();
        assert_eq!(roles("{accent}"), ["accent"]);
        assert_eq!(roles("\"{accent}\" {base}"), ["accent", "base"]);
        assert!(roles("bold").is_empty());
        assert!(roles("{accent").is_empty());
    }
}
