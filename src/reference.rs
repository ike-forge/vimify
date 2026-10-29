use std::path::{Path, PathBuf};
use std::{fs, io};

use anyhow::{Context, Result};
use serde::Deserialize;
use toml_edit::{Array, DocumentMut, value};

use crate::{fsx, paths};

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Reference {
    pub scan: Scan,
    /// Ports the user unchecked; they start unchecked on later runs.
    pub exclusions: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Scan {
    pub extra_paths: Vec<String>,
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
        Some(text) => toml::from_str(&text).with_context(|| format!("{}", path.display())),
        None => Ok(Reference::default()),
    }
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
