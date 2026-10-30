use std::fs;
use std::path::{Path, PathBuf};

use crate::edit;
use crate::port::Port;
use crate::state::{self, Applied, State};

#[derive(Debug, PartialEq, Eq)]
pub struct Selection {
    pub selected: Vec<String>,
    pub excluded: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Op {
    pub tool: String,
    pub action: Action,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Apply {
        path: PathBuf,
    },
    /// Target file does not exist yet.
    Create {
        path: PathBuf,
    },
    /// Applied with an older port version, a different reference, or a key the user
    /// changed by hand.
    Update {
        path: PathBuf,
        manual: bool,
    },
    Revert {
        path: PathBuf,
        manual: bool,
    },
    Current {
        path: PathBuf,
    },
    /// Port has no editor or mappings yet.
    Unsupported,
}

impl Action {
    pub fn label(&self) -> &'static str {
        match self {
            Action::Apply { .. } => "apply",
            Action::Create { .. } => "create",
            Action::Update { .. } => "update",
            Action::Revert { .. } => "revert",
            Action::Current { .. } => "current",
            Action::Unsupported => "unsupported",
        }
    }

    pub fn path(&self) -> Option<&Path> {
        match self {
            Action::Apply { path }
            | Action::Create { path }
            | Action::Update { path, .. }
            | Action::Revert { path, .. }
            | Action::Current { path } => Some(path),
            Action::Unsupported => None,
        }
    }

    /// The user changed a key vimify owns, and confirming overrides that change.
    pub fn manual(&self) -> bool {
        matches!(
            self,
            Action::Update { manual: true, .. } | Action::Revert { manual: true, .. }
        )
    }

    pub fn writes(&self) -> bool {
        !matches!(self, Action::Current { .. } | Action::Unsupported)
    }
}

/// Checkboxes are desired state; diffing against `state` yields the ops.
pub fn plan(selection: &Selection, ports: &[Port], state: &State, reference_hash: &str) -> Vec<Op> {
    let selected = selection.selected.iter().filter_map(|tool| {
        let port = ports.iter().find(|p| &p.name == tool)?;
        let action = match (state.tools.get(tool), &port.target) {
            (Some(a), _) => {
                let manual = edited_by_hand(a);
                if manual || a.port_version != port.version || a.reference_hash != reference_hash {
                    Action::Update {
                        path: a.path.clone(),
                        manual,
                    }
                } else {
                    Action::Current {
                        path: a.path.clone(),
                    }
                }
            }
            (None, Some(target)) if port.writable() => match target.resolve() {
                Some((path, true)) => Action::Apply { path },
                Some((path, false)) => Action::Create { path },
                None => Action::Unsupported,
            },
            (None, _) => Action::Unsupported,
        };
        Some(Op {
            tool: tool.clone(),
            action,
        })
    });
    let reverted = selection.excluded.iter().filter_map(|tool| {
        let applied = state.tools.get(tool)?;
        Some(Op {
            tool: tool.clone(),
            action: Action::Revert {
                path: applied.path.clone(),
                manual: edited_by_hand(applied),
            },
        })
    });
    selected.chain(reverted).collect()
}

/// Whether an owned key differs from what vimify wrote. Edits to anything else do not
/// count, nor does a tool rewriting its own config. A file vimify cannot read counts.
fn edited_by_hand(applied: &Applied) -> bool {
    let Ok(text) = fs::read_to_string(&applied.path) else {
        return true;
    };
    if state::hash(text.as_bytes()) == applied.file_hash {
        return false;
    }
    let Ok(editor) = edit::open(applied.format, &text) else {
        return true;
    };
    applied.owned.iter().any(|(setting, value)| {
        let path: Vec<&str> = setting.split('.').collect();
        editor.get(&path).as_ref() != Some(value)
    })
}

/// Exclusions for tools not detected this run are kept.
pub fn exclusions(previous: &[String], selection: &Selection) -> Vec<String> {
    let mut out: Vec<String> = previous
        .iter()
        .filter(|e| !selection.selected.contains(e))
        .chain(&selection.excluded)
        .cloned()
        .collect();
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Notation;
    use crate::edit::Value;
    use crate::port::{Format, Strategy, Target};
    use std::collections::BTreeMap;
    use tempfile::TempDir;

    const REF: &str = "ref";

    fn port(name: &str, version: &str, paths: Option<Vec<String>>) -> Port {
        Port {
            name: name.into(),
            version: version.into(),
            binaries: vec![name.into()],
            target: paths.map(|paths| Target {
                format: Format::Yaml,
                strategy: Strategy::Edit,
                key_notation: Some(Notation::Angle),
                paths,
            }),
            keys: BTreeMap::from([("a.b".into(), "down".into())]),
            theme: BTreeMap::new(),
        }
    }

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn selection(selected: &[&str], excluded: &[&str]) -> Selection {
        Selection {
            selected: strings(selected),
            excluded: strings(excluded),
        }
    }

    const CONFIG: &str = "a:\n  b: j\nc: 1\n";

    /// `tool` applied at `version`, with `CONFIG` on disk as vimify wrote it.
    fn applied(dir: &TempDir, tool: &str, version: &str) -> (State, PathBuf) {
        let path = dir.path().join("config.yml");
        fs::write(&path, CONFIG).unwrap();
        let mut state = State::default();
        state.tools.insert(
            tool.into(),
            Applied {
                port_version: version.into(),
                path: path.clone(),
                backup: None,
                reference_hash: REF.into(),
                file_hash: state::hash(CONFIG.as_bytes()),
                format: Format::Yaml,
                owned: BTreeMap::from([("a.b".into(), Value::Str("j".into()))]),
            },
        );
        (state, path)
    }

    fn actions(ops: Vec<Op>) -> Vec<(String, Action)> {
        ops.into_iter().map(|op| (op.tool, op.action)).collect()
    }

    #[test]
    fn existing_file_is_applied_missing_file_is_created() {
        let dir = TempDir::new().unwrap();
        let existing = dir.path().join("existing.yml");
        fs::write(&existing, "").unwrap();
        let missing = dir.path().join("missing.yml");
        let ports = [
            port("a", "1", Some(vec![existing.to_str().unwrap().into()])),
            port("b", "1", Some(vec![missing.to_str().unwrap().into()])),
        ];

        let ops = plan(&selection(&["a", "b"], &[]), &ports, &State::default(), REF);

        assert_eq!(
            actions(ops),
            [
                ("a".into(), Action::Apply { path: existing }),
                ("b".into(), Action::Create { path: missing }),
            ]
        );
    }

    #[test]
    fn first_existing_path_wins_over_first_listed() {
        let dir = TempDir::new().unwrap();
        let first = dir.path().join("first.yml");
        let second = dir.path().join("second.yml");
        fs::write(&second, "").unwrap();
        let paths = vec![
            first.to_str().unwrap().into(),
            second.to_str().unwrap().into(),
        ];

        let ops = plan(
            &selection(&["a"], &[]),
            &[port("a", "1", Some(paths))],
            &State::default(),
            REF,
        );

        assert_eq!(ops[0].action, Action::Apply { path: second });
    }

    #[test]
    fn port_without_target_is_unsupported() {
        let ops = plan(
            &selection(&["a"], &[]),
            &[port("a", "1", None)],
            &State::default(),
            REF,
        );
        assert_eq!(ops[0].action, Action::Unsupported);
    }

    #[test]
    fn port_vimify_cannot_write_is_unsupported() {
        let mut unmapped = port("a", "1", Some(vec!["/c".into()]));
        unmapped.keys.clear();
        let mut no_editor = port("b", "1", Some(vec!["/c".into()]));
        no_editor.target.as_mut().unwrap().format = Format::Toml;

        let ops = plan(
            &selection(&["a", "b"], &[]),
            &[unmapped, no_editor],
            &State::default(),
            REF,
        );

        assert_eq!(ops[0].action, Action::Unsupported);
        assert_eq!(ops[1].action, Action::Unsupported);
    }

    fn update(path: &Path, manual: bool) -> Action {
        Action::Update {
            path: path.to_owned(),
            manual,
        }
    }

    fn revert(path: &Path, manual: bool) -> Action {
        Action::Revert {
            path: path.to_owned(),
            manual,
        }
    }

    fn one(tool_selected: bool, state: &State, reference: &str) -> Action {
        let names: &[&str] = &["a"];
        let sel = if tool_selected {
            selection(names, &[])
        } else {
            selection(&[], names)
        };
        let mut ops = plan(&sel, &[port("a", "2", None)], state, reference);
        ops.remove(0).action
    }

    #[test]
    fn applied_tool_is_current_or_updated() {
        let dir = TempDir::new().unwrap();
        let (same, path) = applied(&dir, "a", "2");
        let (older, _) = applied(&dir, "a", "1");

        assert_eq!(
            one(true, &same, REF),
            Action::Current { path: path.clone() }
        );
        assert_eq!(one(true, &older, REF), update(&path, false));
    }

    #[test]
    fn changed_reference_updates_applied_tool() {
        let dir = TempDir::new().unwrap();
        let (state, path) = applied(&dir, "a", "2");
        assert_eq!(one(true, &state, "other"), update(&path, false));
    }

    #[test]
    fn edit_of_an_unowned_key_is_not_manual() {
        let dir = TempDir::new().unwrap();
        let (state, path) = applied(&dir, "a", "2");
        fs::write(&path, "a:\n  b: j\nc: 2 # mine\n").unwrap();

        assert_eq!(
            one(true, &state, REF),
            Action::Current { path: path.clone() }
        );
        assert_eq!(one(false, &state, REF), revert(&path, false));
    }

    #[test]
    fn edit_of_an_owned_key_is_manual() {
        let dir = TempDir::new().unwrap();
        let (state, path) = applied(&dir, "a", "2");
        fs::write(&path, "a:\n  b: x\nc: 1\n").unwrap();

        assert_eq!(one(true, &state, REF), update(&path, true));
        assert_eq!(one(false, &state, REF), revert(&path, true));
    }

    #[test]
    fn removing_an_owned_key_is_manual() {
        let dir = TempDir::new().unwrap();
        let (state, path) = applied(&dir, "a", "2");
        fs::write(&path, "c: 1\n").unwrap();

        assert_eq!(one(true, &state, REF), update(&path, true));
    }

    #[test]
    fn deleted_or_broken_file_is_manual() {
        let dir = TempDir::new().unwrap();
        let (state, path) = applied(&dir, "a", "2");

        fs::write(&path, "- not a mapping\n").unwrap();
        assert_eq!(one(true, &state, REF), update(&path, true));
        fs::remove_file(&path).unwrap();
        assert_eq!(one(true, &state, REF), update(&path, true));
    }

    #[test]
    fn excluded_tool_is_reverted_only_if_applied() {
        let dir = TempDir::new().unwrap();
        let (state, path) = applied(&dir, "a", "1");
        let ports = [port("a", "1", None), port("b", "1", None)];

        let ops = plan(&selection(&[], &["a", "b"]), &ports, &state, REF);

        assert_eq!(actions(ops), [("a".into(), revert(&path, false))]);
    }

    #[test]
    fn exclusions_keep_undetected_and_drop_reselected() {
        let previous = strings(&["gone", "back"]);
        let out = exclusions(&previous, &selection(&["back"], &["new"]));
        assert_eq!(out, strings(&["gone", "new"]));
    }
}
