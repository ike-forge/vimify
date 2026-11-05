use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{fs, io};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::edit::Value;
use crate::port::{Format, Strategy};
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
    pub reference_hash: String,
    pub files: Vec<AppliedFile>,
}

impl Applied {
    pub fn paths(&self) -> Vec<PathBuf> {
        self.files.iter().map(|f| f.path.clone()).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedFile {
    pub path: PathBuf,
    /// Revert target. `None` means vimify created the file, so revert deletes it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup: Option<PathBuf>,
    pub file_hash: String,
    /// Kept here so a tool can be reverted after its port is gone.
    pub format: Format,
    pub strategy: Strategy,
    /// Dotted setting → value vimify wrote. Telling these from the file's current
    /// values separates manual edits of owned keys from edits of anything else.
    pub owned: BTreeMap<String, Value>,
}

impl State {
    /// A re-apply keeps the first apply's backup: later ones hold vimify's own output,
    /// and revert must reach the pre-vimify file. Keys and files it no longer writes stay
    /// owned, or revert would leave them behind.
    pub fn record(&mut self, tool: &str, mut applied: Applied) {
        if let Some(previous) = self.tools.get(tool) {
            for old in &previous.files {
                match applied.files.iter_mut().find(|f| f.path == old.path) {
                    Some(file) => {
                        file.backup = old.backup.clone();
                        for (setting, value) in &old.owned {
                            file.owned
                                .entry(setting.clone())
                                .or_insert_with(|| value.clone());
                        }
                    }
                    None => applied.files.push(old.clone()),
                }
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

    fn file(path: &str, backup: Option<&str>) -> AppliedFile {
        AppliedFile {
            path: path.into(),
            backup: backup.map(Into::into),
            file_hash: "file".into(),
            format: Format::Yaml,
            strategy: Strategy::Edit,
            owned: BTreeMap::from([
                ("a.b".into(), Value::Str("j".into())),
                ("a.c".into(), Value::List(vec!["x".into()])),
            ]),
        }
    }

    fn with(files: Vec<AppliedFile>) -> Applied {
        Applied {
            port_version: "0.1.0".into(),
            reference_hash: "ref".into(),
            files,
        }
    }

    fn applied(backup: Option<&str>) -> Applied {
        with(vec![file("/c/config.yml", backup)])
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
            state.tools["a"].files[0].backup.as_deref(),
            Some(Path::new("/c/config.yml.bak"))
        );
    }

    #[test]
    fn reapply_of_created_file_stays_without_backup() {
        let mut state = State::default();
        state.record("a", applied(None));
        state.record("a", applied(Some("/c/config.yml.bak")));

        assert_eq!(state.tools["a"].files[0].backup, None);
    }

    #[test]
    fn reapply_keeps_keys_it_no_longer_writes() {
        let mut state = State::default();
        state.record("a", applied(None));
        let mut newer = file("/c/config.yml", None);
        newer.owned = BTreeMap::from([("a.b".into(), Value::Str("n".into()))]);

        state.record("a", with(vec![newer]));

        let owned = &state.tools["a"].files[0].owned;
        assert_eq!(owned["a.b"], Value::Str("n".into()));
        assert!(owned.contains_key("a.c"));
    }

    #[test]
    fn reapply_keeps_files_it_no_longer_writes() {
        let mut state = State::default();
        state.record("a", applied(Some("/c/config.yml.bak")));

        state.record("a", with(vec![file("/c/new.yml", None)]));

        assert_eq!(
            state.tools["a"].paths(),
            [Path::new("/c/new.yml"), Path::new("/c/config.yml")]
        );
    }

    #[test]
    fn each_file_keeps_its_own_backup() {
        let mut state = State::default();
        state.record(
            "a",
            with(vec![
                file("/c/a.yml", Some("/c/a.yml.bak")),
                file("/c/b.yml", None),
            ]),
        );

        state.record(
            "a",
            with(vec![
                file("/c/a.yml", Some("/c/a.yml.bak.1")),
                file("/c/b.yml", Some("/c/b.yml.bak")),
            ]),
        );

        let files = &state.tools["a"].files;
        assert_eq!(files[0].backup.as_deref(), Some(Path::new("/c/a.yml.bak")));
        assert_eq!(files[1].backup, None);
    }

    #[test]
    fn record_replaces_the_other_fields() {
        let mut state = State::default();
        state.record("a", applied(None));
        let mut newer = applied(None);
        newer.files[0].file_hash = "newer".into();
        newer.reference_hash = "new".into();

        state.record("a", newer);

        assert_eq!(state.tools["a"].files[0].file_hash, "newer");
        assert_eq!(state.tools["a"].reference_hash, "new");
    }

    #[test]
    fn hash_is_stable() {
        assert_eq!(
            hash(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
