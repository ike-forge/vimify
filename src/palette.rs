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
    "base",
    "surface",
];

#[derive(Debug, Clone)]
pub struct Palette(BTreeMap<String, String>);

impl Palette {
    pub fn role(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str)
    }
}

/// Roles a port value refers to: `"{accent}"` has `accent`, `bold` has none.
pub fn placeholders(text: &str) -> impl Iterator<Item = &str> {
    text.split('{')
        .skip(1)
        .filter_map(|rest| rest.split_once('}'))
        .map(|(role, _)| role)
}

pub fn load(theme: &str) -> Result<Palette> {
    let text = EMBEDDED
        .get_file(format!("{theme}.toml"))
        .and_then(|f| f.contents_utf8())
        .with_context(|| format!("unknown theme {theme}"))?;
    parse(text).with_context(|| format!("theme {theme}"))
}

fn parse(text: &str) -> Result<Palette> {
    let colors: BTreeMap<String, String> = toml::from_str(text)?;
    for role in ROLES {
        if !colors.contains_key(*role) {
            bail!("missing role {role}");
        }
    }
    for (role, color) in &colors {
        if !ROLES.contains(&role.as_str()) {
            bail!("unknown role {role}");
        }
        if !is_hex(color) {
            bail!("{role}: {color} is not a #rrggbb colour");
        }
    }
    Ok(Palette(colors))
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
        let err = parse(&format!("{FULL}new = \"#000000\"\n")).unwrap_err();
        assert!(err.to_string().contains("new"));
    }

    #[test]
    fn invalid_colour_is_an_error() {
        let err = parse(&FULL.replace("#8caaee", "blue")).unwrap_err();
        assert!(err.to_string().contains("blue"));
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
