use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{fs, io};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use toml_edit::{Array, DocumentMut, table, value};

use crate::palette::{self, Palette};
use crate::{actions, fsx, paths, state};

const DEFAULT_THEME: &str = "catppuccin-frappe";

/// Tools whose port was removed: still accepted in a saved `tools`, then dropped.
const RETIRED: &[&str] = &["tmux"];

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Reference {
    pub theme: String,
    /// One of the theme's named accents; unset keeps the theme's own.
    pub accent: Option<String>,
    pub keys: BTreeMap<String, String>,
    pub scan: Scan,
    /// Unset until the first selection is saved.
    pub tools: Option<Tools>,
}

/// The tools vimify manages. Opt-in: a tool not listed is never written.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "RawTools")]
pub enum Tools {
    /// Every supported tool, including ones installed or supported later.
    All,
    Only(Vec<String>),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawTools {
    Word(String),
    List(Vec<String>),
}

impl TryFrom<RawTools> for Tools {
    type Error = String;

    fn try_from(raw: RawTools) -> Result<Self, String> {
        match raw {
            RawTools::Word(w) if w == "all" => Ok(Tools::All),
            RawTools::Word(w) => Err(format!("tools: expected \"all\" or a list, got \"{w}\"")),
            RawTools::List(list) => Ok(Tools::Only(list)),
        }
    }
}

impl Tools {
    pub fn contains(&self, tool: &str) -> bool {
        match self {
            Tools::All => true,
            Tools::Only(list) => list.iter().any(|t| t == tool),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Scan {
    pub extra_paths: Vec<String>,
}

impl Default for Reference {
    /// A first run has no config and must still have something to apply.
    fn default() -> Self {
        Self {
            theme: DEFAULT_THEME.to_owned(),
            accent: None,
            keys: default_keys(),
            scan: Scan::default(),
            tools: None,
        }
    }
}

fn default_keys() -> BTreeMap<String, String> {
    actions::DEFAULTS
        .iter()
        .map(|(action, key)| ((*action).to_owned(), (*key).to_owned()))
        .collect()
}

#[derive(Serialize)]
struct Hashed<'a> {
    theme: &'a str,
    // Unset leaves the hash as it was before accents, so no tool re-applies for them.
    #[serde(skip_serializing_if = "Option::is_none")]
    accent: Option<&'a str>,
    keys: &'a BTreeMap<String, String>,
}

impl Reference {
    /// Only what reaches tool configs: vimify rewrites `tools` in this same file,
    /// so hashing the file would re-apply every tool after each selection change.
    pub fn hash(&self) -> Result<String> {
        let text = toml::to_string(&Hashed {
            theme: &self.theme,
            accent: self.accent.as_deref(),
            keys: &self.keys,
        })?;
        Ok(state::hash(text.as_bytes()))
    }

    pub fn palette(&self) -> Result<Palette> {
        palette::load(&self.theme)?.with_accent(self.accent.as_deref())
    }
}

fn path() -> Option<PathBuf> {
    paths::expand("$XDG_CONFIG_HOME/vimify/config.toml")
}

/// Missing file is not an error: a first run has none.
pub fn load() -> Result<Reference> {
    let Some(path) = path() else {
        return Ok(Reference::default());
    };
    match read(&path)? {
        Some(text) => parse(&text).with_context(|| format!("{}", path.display())),
        None => Ok(Reference::default()),
    }
}

/// For unattended runs, which apply only to tools the user chose: `tools` must be set.
/// Unknown names fail, so a typo does not silently leave a tool out.
pub fn load_saved(known: &[&str]) -> Result<Reference> {
    let path = path().context("cannot resolve vimify config path")?;
    let text = read(&path)?.with_context(|| format!("{} not found", path.display()))?;
    parse_saved(&text, known).with_context(|| format!("{}", path.display()))
}

fn parse_saved(text: &str, known: &[&str]) -> Result<Reference> {
    let doc: DocumentMut = text.parse()?;
    if !doc.contains_key("tools") {
        bail!(
            "tools not set: run `vimify apply` once, or set `tools = \"all\"` to apply to every tool"
        );
    }
    let mut reference = parse(text)?;
    if let Some(Tools::Only(list)) = &mut reference.tools {
        list.retain(|t| !RETIRED.contains(&t.as_str()));
        if let Some(tool) = list.iter().find(|t| !known.contains(&t.as_str())) {
            bail!("tools: unknown tool {tool}");
        }
    }
    Ok(reference)
}

/// A partial `[keys]` rebinds those actions; the rest keep their defaults.
fn parse(text: &str) -> Result<Reference> {
    let mut reference: Reference = toml::from_str(text)?;
    for action in reference.keys.keys() {
        if !actions::is_action(action) {
            bail!("unknown action {action}");
        }
    }
    for (action, key) in default_keys() {
        reference.keys.entry(action).or_insert(key);
    }
    Ok(reference)
}

/// Rewrites only `tools`; the rest of the user's file is untouched. An empty list is
/// still written: `load_saved` requires the key.
pub fn save_tools(tools: &Tools) -> Result<()> {
    update(|text| with_tools(text, tools))
}

/// Writes only the theme and keys that differ from `before`, so defaults the user
/// never set stay out of the file. A change back to a default removes the setting,
/// so the user follows later changes to the defaults.
pub fn save_settings(before: &Reference, after: &Reference) -> Result<()> {
    update(|text| with_settings(text, before, after))
}

fn update(edit: impl FnOnce(&str) -> Result<String>) -> Result<()> {
    let path = path().context("cannot resolve vimify config path")?;
    let text = read(&path)?.unwrap_or_default();
    let updated = edit(&text).with_context(|| format!("{}", path.display()))?;
    if updated != text {
        fsx::write_atomic(&path, &updated).with_context(|| format!("{}", path.display()))?;
    }
    Ok(())
}

fn read(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("{}", path.display())),
    }
}

fn with_tools(text: &str, tools: &Tools) -> Result<String> {
    let mut doc: DocumentMut = text.parse()?;
    doc["tools"] = match tools {
        Tools::All => value("all"),
        Tools::Only(list) => value(list.iter().map(String::as_str).collect::<Array>()),
    };
    Ok(doc.to_string())
}

fn with_settings(text: &str, before: &Reference, after: &Reference) -> Result<String> {
    let mut doc: DocumentMut = text.parse()?;
    if after.theme != before.theme {
        if after.theme == DEFAULT_THEME {
            doc.remove("theme");
        } else {
            doc["theme"] = value(after.theme.as_str());
        }
    }
    let defaults = default_keys();
    for (action, key) in &after.keys {
        if before.keys.get(action) == Some(key) {
            continue;
        }
        if defaults.get(action) == Some(key) {
            if let Some(keys) = doc.get_mut("keys").and_then(|k| k.as_table_like_mut()) {
                keys.remove(action);
            }
            continue;
        }
        let keys = doc
            .entry("keys")
            .or_insert(table())
            .as_table_like_mut()
            .context("keys is not a table")?;
        keys.insert(action, value(key.as_str()));
    }
    Ok(doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn hash(text: &str) -> String {
        parse(text).unwrap().hash().unwrap()
    }

    #[test]
    fn empty_config_gets_the_defaults() {
        let reference = parse("").unwrap();
        assert_eq!(reference.theme, DEFAULT_THEME);
        assert_eq!(reference.keys["down"], "j");
    }

    #[test]
    fn an_accent_changes_the_hash_and_the_palette() {
        assert_ne!(hash("accent = \"mauve\"\n"), hash(""));
        let reference = parse("accent = \"mauve\"\n").unwrap();
        assert_eq!(reference.palette().unwrap().role("accent"), Some("#ca9ee6"));
    }

    #[test]
    fn partial_keys_keep_the_other_defaults() {
        let reference = parse("[keys]\ndown = \"n\"\n").unwrap();
        assert_eq!(reference.keys["down"], "n");
        assert_eq!(reference.keys["up"], "k");
    }

    #[test]
    fn unknown_action_is_an_error() {
        let err = parse("[keys]\nflyy = \"f\"\n").unwrap_err();
        assert!(err.to_string().contains("unknown action flyy"));
    }

    #[test]
    fn hash_ignores_everything_but_theme_and_keys() {
        let base = "theme = \"a\"\n[keys]\ndown = \"j\"\n";
        let noisy = "theme = \"a\"\ntools = [\"btop\"]\n[keys]\ndown = \"j\"\n[scan]\nextra_paths = [\"~/bin\"]\n";
        assert_eq!(hash(base), hash(noisy));
    }

    #[test]
    fn hash_ignores_key_order() {
        let one = "[keys]\ndown = \"j\"\nup = \"k\"\n";
        let other = "[keys]\nup = \"k\"\ndown = \"j\"\n";
        assert_eq!(hash(one), hash(other));
    }

    #[test]
    fn hash_changes_with_theme_or_keys() {
        let base = hash("theme = \"a\"\n[keys]\ndown = \"j\"\n");
        assert_ne!(base, hash("theme = \"b\"\n[keys]\ndown = \"j\"\n"));
        assert_ne!(base, hash("theme = \"a\"\n[keys]\ndown = \"n\"\n"));
    }

    #[test]
    fn empty_reference_hashes() {
        assert_eq!(hash(""), hash("tools = []\n"));
    }

    fn only(v: &[&str]) -> Tools {
        Tools::Only(strings(v))
    }

    #[test]
    fn preserves_rest_of_file() {
        let text = "# my theme\ntheme = \"catppuccin-frappe\"\n\n[scan]\nextra_paths = [\"~/bin\"] # mine\n";
        let out = with_tools(text, &only(&["btop"])).unwrap();
        assert_eq!(
            out,
            "# my theme\ntheme = \"catppuccin-frappe\"\ntools = [\"btop\"]\n\n[scan]\nextra_paths = [\"~/bin\"] # mine\n"
        );
    }

    #[test]
    fn replaces_existing_tools() {
        let out = with_tools("tools = [\"a\"]\n", &only(&["b"])).unwrap();
        assert_eq!(out, "tools = [\"b\"]\n");
        let out = with_tools("tools = [\"a\"]\n", &Tools::All).unwrap();
        assert_eq!(out, "tools = \"all\"\n");
    }

    #[test]
    fn no_tools_still_saves_the_selection() {
        assert_eq!(with_tools("", &only(&[])).unwrap(), "tools = []\n");
    }

    const KNOWN: &[&str] = &["btop", "lazygit"];

    #[test]
    fn saved_config_loads() {
        let reference = parse_saved("tools = [\"btop\"]\n", KNOWN).unwrap();
        assert_eq!(reference.tools, Some(only(&["btop"])));
        let reference = parse_saved("tools = \"all\"\n", KNOWN).unwrap();
        assert_eq!(reference.tools, Some(Tools::All));
    }

    #[test]
    fn saved_config_needs_tools() {
        let err = parse_saved("theme = \"a\"\n", KNOWN).unwrap_err();
        assert!(err.to_string().contains("tools not set"));
    }

    #[test]
    fn saved_config_accepts_empty_tools() {
        let reference = parse_saved("tools = []\n", KNOWN).unwrap();
        assert_eq!(reference.tools, Some(only(&[])));
    }

    #[test]
    fn saved_config_rejects_malformed_tools() {
        assert!(parse_saved("tools = \"btop\"\n", KNOWN).is_err());
        assert!(parse_saved("tools = [1]\n", KNOWN).is_err());
        assert!(parse_saved("tools = [\n", KNOWN).is_err());
    }

    #[test]
    fn saved_config_drops_retired_tools() {
        let reference = parse_saved("tools = [\"tmux\", \"btop\"]\n", KNOWN).unwrap();
        assert_eq!(reference.tools, Some(only(&["btop"])));
    }

    #[test]
    fn saved_config_rejects_unknown_tools() {
        let err = parse_saved("tools = [\"btpo\"]\n", KNOWN).unwrap_err();
        assert!(err.to_string().contains("unknown tool btpo"));
    }

    fn edited(text: &str, edit: impl FnOnce(&mut Reference)) -> String {
        let before = parse(text).unwrap();
        let mut after = before.clone();
        edit(&mut after);
        with_settings(text, &before, &after).unwrap()
    }

    #[test]
    fn settings_write_only_what_changed() {
        let text = "# mine\ntheme = \"catppuccin-frappe\" # keep\n\n[keys]\nup = \"k\"\n";
        let out = edited(text, |r| {
            r.keys.insert("down".into(), "n".into());
        });
        assert_eq!(
            out,
            "# mine\ntheme = \"catppuccin-frappe\" # keep\n\n[keys]\nup = \"k\"\ndown = \"n\"\n"
        );
    }

    #[test]
    fn settings_create_keys_table_and_theme() {
        let out = edited("tools = []\n", |r| {
            r.theme = "other".into();
            r.keys.insert("quit".into(), "Q".into());
        });
        assert_eq!(
            out,
            "tools = []\ntheme = \"other\"\n\n[keys]\nquit = \"Q\"\n"
        );
    }

    #[test]
    fn settings_back_to_defaults_are_removed() {
        let text = "theme = \"other\"\n\n[keys]\ndown = \"n\"\nup = \"i\" # mine\n";
        let out = edited(text, |r| {
            r.theme = DEFAULT_THEME.into();
            r.keys.insert("down".into(), "j".into());
        });
        assert_eq!(out, "\n[keys]\nup = \"i\" # mine\n");
    }

    #[test]
    fn unchanged_settings_leave_file_untouched() {
        assert_eq!(edited("", |_| {}), "");
    }
}
