use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{fs, io};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::edit::Value;
use crate::port::Format;
use crate::{fsx, paths};

/// What vimify has applied, per port.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub tools: BTreeMap<String, Applied>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Applied {
    pub port_version: String,
    pub path: PathBuf,
    /// Revert target. `None` means vimify created the file, so revert deletes it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup: Option<PathBuf>,
    pub reference_hash: String,
    pub file_hash: String,
    /// Kept here so a tool can be reverted after its port is gone.
    pub format: Format,
    /// Dotted setting → value vimify wrote. Telling these from the file's current
    /// values separates manual edits of owned keys from edits of anything else.
    pub owned: BTreeMap<String, Value>,
}

impl State {
    /// A re-apply keeps the first apply's backup: later ones hold vimify's own output,
    /// and revert must reach the pre-vimify file. Keys it no longer writes stay owned,
    /// or revert would leave them behind.
    pub fn record(&mut self, tool: &str, mut applied: Applied) {
        if let Some(previous) = self.tools.get(tool) {
            applied.backup = previous.backup.clone();
            for (setting, value) in &previous.owned {
                applied
                    .owned
                    .entry(setting.clone())
                    .or_insert_with(|| value.clone());
            }
        }
        self.tools.insert(tool.to_owned(), applied);
    }
}

/// Stored in `state.toml`, so it must stay stable across builds.
pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn path() -> Option<PathBuf> {
    paths::expand("$XDG_STATE_HOME/vimify/state.toml")
}

/// Missing file means nothing applied yet.
pub fn load() -> Result<State> {
    match path() {
        Some(path) => load_from(&path),
        None => Ok(State::default()),
    }
}

pub fn save(state: &State) -> Result<()> {
    let path = path().context("cannot resolve vimify state path")?;
    save_to(&path, state)
}

fn load_from(path: &Path) -> Result<State> {
    match fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text).with_context(|| format!("{}", path.display())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(State::default()),
        Err(e) => Err(e).with_context(|| format!("{}", path.display())),
    }
}

fn save_to(path: &Path, state: &State) -> Result<()> {
    let text = toml::to_string(state).with_context(|| format!("{}", path.display()))?;
    fsx::write_atomic(path, &text).with_context(|| format!("{}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn applied(backup: Option<&str>) -> Applied {
        Applied {
            port_version: "0.1.0".into(),
            path: "/c/config.yml".into(),
            backup: backup.map(Into::into),
            reference_hash: "ref".into(),
            file_hash: "file".into(),
            format: Format::Yaml,
            owned: BTreeMap::from([
                ("a.b".into(), Value::Str("j".into())),
                ("a.c".into(), Value::List(vec!["x".into()])),
            ]),
        }
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("vimify/state.toml");
        let mut state = State::default();
        state.record("backed-up", applied(Some("/c/config.yml.bak")));
        state.record("created", applied(None));

        save_to(&path, &state).unwrap();
        let loaded = load_from(&path).unwrap();

        assert_eq!(loaded.tools, state.tools);
    }

    #[test]
    fn missing_file_is_empty_state() {
        let dir = TempDir::new().unwrap();
        let state = load_from(&dir.path().join("state.toml")).unwrap();
        assert!(state.tools.is_empty());
    }

    #[test]
    fn corrupt_file_is_an_error() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("state.toml");
        fs::write(&path, "tools = 1").unwrap();
        assert!(load_from(&path).is_err());
    }

    #[test]
    fn reapply_keeps_first_backup() {
        let mut state = State::default();
        state.record("a", applied(Some("/c/config.yml.bak")));
        state.record("a", applied(Some("/c/config.yml.bak.1")));

        assert_eq!(
            state.tools["a"].backup.as_deref(),
            Some(Path::new("/c/config.yml.bak"))
        );
    }

    #[test]
    fn reapply_of_created_file_stays_without_backup() {
        let mut state = State::default();
        state.record("a", applied(None));
        state.record("a", applied(Some("/c/config.yml.bak")));

        assert_eq!(state.tools["a"].backup, None);
    }

    #[test]
    fn reapply_keeps_keys_it_no_longer_writes() {
        let mut state = State::default();
        state.record("a", applied(None));
        let newer = Applied {
            owned: BTreeMap::from([("a.b".into(), Value::Str("n".into()))]),
            ..applied(None)
        };

        state.record("a", newer);

        assert_eq!(state.tools["a"].owned["a.b"], Value::Str("n".into()));
        assert!(state.tools["a"].owned.contains_key("a.c"));
    }

    #[test]
    fn record_replaces_the_other_fields() {
        let mut state = State::default();
        state.record("a", applied(None));
        let newer = Applied {
            file_hash: "newer".into(),
            ..applied(None)
        };

        state.record("a", newer);

        assert_eq!(state.tools["a"].file_hash, "newer");
    }

    #[test]
    fn hash_is_stable() {
        assert_eq!(
            hash(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
