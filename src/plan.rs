use std::path::{Path, PathBuf};
use std::{fs, io};

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
    /// Applied with an older port version or a different reference.
    Update {
        path: PathBuf,
    },
    Revert {
        path: PathBuf,
        /// The user edited the file after apply; the confirm screen warns.
        modified: bool,
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
            | Action::Update { path }
            | Action::Revert { path, .. }
            | Action::Current { path } => Some(path),
            Action::Unsupported => None,
        }
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
            (Some(a), _)
                if a.port_version != port.version || a.reference_hash != reference_hash =>
            {
                Action::Update {
                    path: a.path.clone(),
                }
            }
            (Some(a), _) => Action::Current {
                path: a.path.clone(),
            },
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
                modified: is_modified(applied),
            },
        })
    });
    selected.chain(reverted).collect()
}

/// A missing file has no edits to lose. An unreadable one might, so it counts.
fn is_modified(applied: &Applied) -> bool {
    match fs::read(&applied.path) {
        Ok(bytes) => state::hash(&bytes) != applied.file_hash,
        Err(e) => e.kind() != io::ErrorKind::NotFound,
    }
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

    fn applied(tool: &str, version: &str, path: &str) -> State {
        let mut state = State::default();
        state.tools.insert(
            tool.into(),
            Applied {
                port_version: version.into(),
                path: path.into(),
                backup: None,
                reference_hash: REF.into(),
                file_hash: "file".into(),
            },
        );
        state
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

    #[test]
    fn applied_tool_is_current_or_updated() {
        let ports = [port("a", "2", None)];
        let sel = selection(&["a"], &[]);

        let same = plan(&sel, &ports, &applied("a", "2", "/c"), REF);
        let older = plan(&sel, &ports, &applied("a", "1", "/c"), REF);

        assert_eq!(same[0].action, Action::Current { path: "/c".into() });
        assert_eq!(older[0].action, Action::Update { path: "/c".into() });
    }

    #[test]
    fn changed_reference_updates_applied_tool() {
        let ops = plan(
            &selection(&["a"], &[]),
            &[port("a", "1", None)],
            &applied("a", "1", "/c"),
            "other",
        );
        assert_eq!(ops[0].action, Action::Update { path: "/c".into() });
    }

    #[test]
    fn excluded_tool_is_reverted_only_if_applied() {
        let ports = [port("a", "1", None), port("b", "1", None)];
        let ops = plan(
            &selection(&[], &["a", "b"]),
            &ports,
            &applied("a", "1", "/c"),
            REF,
        );
        assert_eq!(
            actions(ops),
            [(
                "a".into(),
                Action::Revert {
                    path: "/c".into(),
                    modified: false,
                }
            )]
        );
    }

    #[test]
    fn revert_is_modified_when_the_file_hash_differs() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yml");
        fs::write(&path, "applied").unwrap();
        let mut state = applied("a", "1", path.to_str().unwrap());
        state.tools.get_mut("a").unwrap().file_hash = state::hash(b"applied");
        let sel = selection(&[], &["a"]);
        let ports = [port("a", "1", None)];

        let clean = plan(&sel, &ports, &state, REF);
        fs::write(&path, "edited").unwrap();
        let edited = plan(&sel, &ports, &state, REF);

        let modified =
            |ops: &[Op]| matches!(ops[0].action, Action::Revert { modified, .. } if modified);
        assert!(!modified(&clean));
        assert!(modified(&edited));
    }

    #[test]
    fn exclusions_keep_undetected_and_drop_reselected() {
        let previous = strings(&["gone", "back"]);
        let out = exclusions(&previous, &selection(&["back"], &["new"]));
        assert_eq!(out, strings(&["gone", "new"]));
    }
}
