use std::{fs, io};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::paths;

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

/// Missing file is not an error: a first run has none.
pub fn load() -> Result<Reference> {
    let Some(path) = paths::expand("$XDG_CONFIG_HOME/vimify/config.toml") else {
        return Ok(Reference::default());
    };
    match fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text).with_context(|| format!("{}", path.display())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Reference::default()),
        Err(e) => Err(e).with_context(|| format!("{}", path.display())),
    }
}
