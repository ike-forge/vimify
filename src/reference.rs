use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{fs, io};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use toml_edit::{Array, DocumentMut, value};

use crate::{actions, fsx, paths, state};

const DEFAULT_THEME: &str = "catppuccin-frappe";

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Reference {
    pub theme: String,
    pub keys: BTreeMap<String, String>,
    pub scan: Scan,
    /// Ports the user unchecked; they start unchecked on later runs.
    pub exclusions: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Scan {
    pub extra_paths: Vec<String>,
}

impl Default for Reference {
    /// A first run has no config and must still have something to apply.
    fn default() -> Self {
        Self {
            theme: DEFAULT_THEME.to_owned(),
            keys: default_keys(),
            scan: Scan::default(),
            exclusions: Vec::new(),
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
    keys: &'a BTreeMap<String, String>,
}

impl Reference {
    /// Only what reaches tool configs: vimify rewrites `exclusions` in this same file,
    /// so hashing the file would re-apply every tool after each selection change.
    pub fn hash(&self) -> Result<String> {
        let text = toml::to_string(&Hashed {
            theme: &self.theme,
            keys: &self.keys,
        })?;
        Ok(state::hash(text.as_bytes()))
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

/// Rewrites only `exclusions`; the rest of the user's file is untouched.
pub fn save_exclusions(exclusions: &[String]) -> Result<()> {
    let path = path().context("cannot resolve vimify config path")?;
    let text = read(&path)?.unwrap_or_default();
    let updated =
        with_exclusions(&text, exclusions).with_context(|| format!("{}", path.display()))?;
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

fn with_exclusions(text: &str, exclusions: &[String]) -> Result<String> {
    let mut doc: DocumentMut = text.parse()?;
    if exclusions.is_empty() && !doc.contains_key("exclusions") {
        return Ok(text.to_owned());
    }
    doc["exclusions"] = value(exclusions.iter().map(String::as_str).collect::<Array>());
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
        let noisy = "theme = \"a\"\nexclusions = [\"btop\"]\n[keys]\ndown = \"j\"\n[scan]\nextra_paths = [\"~/bin\"]\n";
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
        assert_eq!(hash(""), hash("exclusions = []\n"));
    }

    #[test]
    fn preserves_rest_of_file() {
        let text = "# my theme\ntheme = \"catppuccin-frappe\"\n\n[scan]\nextra_paths = [\"~/bin\"] # mine\n";
        let out = with_exclusions(text, &strings(&["btop"])).unwrap();
        assert_eq!(
            out,
            "# my theme\ntheme = \"catppuccin-frappe\"\nexclusions = [\"btop\"]\n\n[scan]\nextra_paths = [\"~/bin\"] # mine\n"
        );
    }

    #[test]
    fn replaces_existing_exclusions() {
        let out = with_exclusions("exclusions = [\"a\"]\n", &strings(&["b"])).unwrap();
        assert_eq!(out, "exclusions = [\"b\"]\n");
    }

    #[test]
    fn no_exclusions_leaves_file_untouched() {
        assert_eq!(with_exclusions("", &[]).unwrap(), "");
    }

    #[test]
    fn clearing_keeps_an_empty_list() {
        let out = with_exclusions("exclusions = [\"a\"]\n", &[]).unwrap();
        assert_eq!(out, "exclusions = []\n");
    }
}
