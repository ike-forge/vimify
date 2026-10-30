use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use include_dir::{Dir, include_dir};

static EMBEDDED: Dir = include_dir!("$CARGO_MANIFEST_DIR/palettes");

pub const ROLES: &[&str] = &["accent", "selection"];

#[derive(Debug, Clone)]
pub struct Palette(BTreeMap<String, String>);

impl Palette {
    pub fn role(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str)
    }
}

/// The role a port value refers to: `{accent}` is `accent`, `bold` is none.
pub fn role_of(value: &str) -> Option<&str> {
    value.strip_prefix('{')?.strip_suffix('}')
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

    #[test]
    fn missing_role_is_an_error() {
        let err = parse("accent = \"#000000\"").unwrap_err();
        assert!(err.to_string().contains("selection"));
    }

    #[test]
    fn unknown_role_is_an_error() {
        let err = parse("accent = \"#000000\"\nselection = \"#000000\"\nnew = \"#000000\"");
        assert!(err.unwrap_err().to_string().contains("new"));
    }

    #[test]
    fn invalid_colour_is_an_error() {
        let err = parse("accent = \"blue\"\nselection = \"#000000\"").unwrap_err();
        assert!(err.to_string().contains("blue"));
    }

    #[test]
    fn role_of_needs_both_braces() {
        assert_eq!(role_of("{accent}"), Some("accent"));
        assert_eq!(role_of("bold"), None);
        assert_eq!(role_of("{accent"), None);
    }
}
