use std::collections::BTreeMap;
use std::path::PathBuf;
use std::{fs, io};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::paths;

/// What vimify has applied, per port.
#[derive(Debug, Default, Deserialize)]
pub struct State {
    #[serde(default)]
    pub tools: BTreeMap<String, Applied>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Applied {
    pub port_version: String,
    pub path: PathBuf,
}

/// Missing file means nothing applied yet.
pub fn load() -> Result<State> {
    let Some(path) = paths::expand("$XDG_STATE_HOME/vimify/state.toml") else {
        return Ok(State::default());
    };
    match fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text).with_context(|| format!("{}", path.display())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(State::default()),
        Err(e) => Err(e).with_context(|| format!("{}", path.display())),
    }
}
