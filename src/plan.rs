use std::fs;
use std::path::PathBuf;

use crate::edit;
use crate::port::{Port, Target};
use crate::state::{self, Applied, AppliedFile, State};

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

/// `paths` of Apply, Create and Update line up with the port's targets.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Apply {
        paths: Vec<PathBuf>,
    },
    /// No target file exists yet.
    Create {
        paths: Vec<PathBuf>,
    },
    /// Applied with an older port version, a different reference, or a key the user
    /// changed by hand.
    Update {
        paths: Vec<PathBuf>,
        manual: bool,
    },
    Revert {
        paths: Vec<PathBuf>,
        manual: bool,
    },
    Current {
        paths: Vec<PathBuf>,
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

    pub fn paths(&self) -> &[PathBuf] {
        match self {
            Action::Apply { paths }
            | Action::Create { paths }
            | Action::Update { paths, .. }
            | Action::Revert { paths, .. }
            | Action::Current { paths } => paths,
            Action::Unsupported => &[],
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
        let action = match state.tools.get(tool) {
            Some(a) => update(a, port, reference_hash),
            None => fresh(port),
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
                paths: applied.paths(),
                manual: edited_by_hand(applied),
            },
        })
    });
    selected.chain(reverted).collect()
}

fn fresh(port: &Port) -> Action {
    if !port.writable() {
        return Action::Unsupported;
    }
    let resolved: Option<Vec<_>> = port.targets.iter().map(|t| t.resolve()).collect();
    let Some(resolved) = resolved else {
        return Action::Unsupported;
    };
    let any_exists = resolved.iter().any(|(_, exists)| *exists);
    let paths = resolved.into_iter().map(|(path, _)| path).collect();
    if any_exists {
        Action::Apply { paths }
    } else {
        Action::Create { paths }
    }
}

fn update(applied: &Applied, port: &Port, reference_hash: &str) -> Action {
    let manual = edited_by_hand(applied);
    if !manual && applied.port_version == port.version && applied.reference_hash == reference_hash {
        return Action::Current {
            paths: applied.paths(),
        };
    }
    if !port.writable() {
        return Action::Unsupported;
    }
    let paths: Option<Vec<_>> = port.targets.iter().map(|t| keep_path(applied, t)).collect();
    match paths {
        Some(paths) => Action::Update { paths, manual },
        None => Action::Unsupported,
    }
}

/// The file already holding this target, else where a target new to the port goes.
fn keep_path(applied: &Applied, target: &Target) -> Option<PathBuf> {
    let candidates = target.candidates();
    let kept = applied.files.iter().find(|f| candidates.contains(&f.path));
    match kept {
        Some(file) => Some(file.path.clone()),
        None => target.resolve().map(|(path, _)| path),
    }
}

/// Whether an owned key differs from what vimify wrote. Edits to anything else do not
/// count, nor does a tool rewriting its own config. A file vimify cannot read counts.
fn edited_by_hand(applied: &Applied) -> bool {
    applied.files.iter().any(file_edited_by_hand)
}

fn file_edited_by_hand(file: &AppliedFile) -> bool {
    let Ok(text) = fs::read_to_string(&file.path) else {
        return true;
    };
    if state::hash(text.as_bytes()) == file.file_hash {
        return false;
    }
    let Ok(editor) = edit::open(file.format, file.strategy, &text) else {
        return true;
    };
    file.owned.iter().any(|(setting, value)| {
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
    use std::path::Path;
    use tempfile::TempDir;

    const REF: &str = "ref";

    fn port(name: &str, version: &str, paths: Option<Vec<String>>) -> Port {
        Port {
            name: name.into(),
            version: version.into(),
            binaries: vec![name.into()],
            versions: semver::VersionReq::STAR,
            version_args: vec![],
            targets: paths.map(target).into_iter().collect(),
            notes: None,
        }
    }

    fn target(paths: Vec<String>) -> Target {
        Target {
            format: Format::Yaml,
            strategy: Strategy::Edit,
            key_notation: Some(Notation::Angle),
            paths,
            keys: BTreeMap::from([("a.b".into(), "down".into())]),
            theme: BTreeMap::new(),
            template: None,
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
                reference_hash: REF.into(),
                files: vec![AppliedFile {
                    path: path.clone(),
                    backup: None,
                    file_hash: state::hash(CONFIG.as_bytes()),
                    format: Format::Yaml,
                    strategy: Strategy::Edit,
                    owned: BTreeMap::from([("a.b".into(), Value::Str("j".into()))]),
                }],
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
                (
                    "a".into(),
                    Action::Apply {
                        paths: vec![existing]
                    }
                ),
                (
                    "b".into(),
                    Action::Create {
                        paths: vec![missing]
                    }
                ),
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

        assert_eq!(
            ops[0].action,
            Action::Apply {
                paths: vec![second]
            }
        );
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
        unmapped.targets[0].keys.clear();
        let mut no_editor = port("b", "1", Some(vec!["/c".into()]));
        no_editor.targets[0].format = Format::Json;

        let ops = plan(
            &selection(&["a", "b"], &[]),
            &[unmapped, no_editor],
            &State::default(),
            REF,
        );

        assert_eq!(ops[0].action, Action::Unsupported);
        assert_eq!(ops[1].action, Action::Unsupported);
    }

    fn updated(path: &Path, manual: bool) -> Action {
        Action::Update {
            paths: vec![path.to_owned()],
            manual,
        }
    }

    fn revert(path: &Path, manual: bool) -> Action {
        Action::Revert {
            paths: vec![path.to_owned()],
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
        let path = state.tools["a"].files[0].path.to_str().unwrap().to_owned();
        let port = port("a", "2", Some(vec![path]));
        let mut ops = plan(&sel, &[port], state, reference);
        ops.remove(0).action
    }

    #[test]
    fn applied_tool_is_current_or_updated() {
        let dir = TempDir::new().unwrap();
        let (same, path) = applied(&dir, "a", "2");
        let (older, _) = applied(&dir, "a", "1");

        assert_eq!(
            one(true, &same, REF),
            Action::Current {
                paths: vec![path.clone()]
            }
        );
        assert_eq!(one(true, &older, REF), updated(&path, false));
    }

    #[test]
    fn changed_reference_updates_applied_tool() {
        let dir = TempDir::new().unwrap();
        let (state, path) = applied(&dir, "a", "2");
        assert_eq!(one(true, &state, "other"), updated(&path, false));
    }

    #[test]
    fn edit_of_an_unowned_key_is_not_manual() {
        let dir = TempDir::new().unwrap();
        let (state, path) = applied(&dir, "a", "2");
        fs::write(&path, "a:\n  b: j\nc: 2 # mine\n").unwrap();

        assert_eq!(
            one(true, &state, REF),
            Action::Current {
                paths: vec![path.clone()]
            }
        );
        assert_eq!(one(false, &state, REF), revert(&path, false));
    }

    #[test]
    fn edit_of_an_owned_key_is_manual() {
        let dir = TempDir::new().unwrap();
        let (state, path) = applied(&dir, "a", "2");
        fs::write(&path, "a:\n  b: x\nc: 1\n").unwrap();

        assert_eq!(one(true, &state, REF), updated(&path, true));
        assert_eq!(one(false, &state, REF), revert(&path, true));
    }

    #[test]
    fn removing_an_owned_key_is_manual() {
        let dir = TempDir::new().unwrap();
        let (state, path) = applied(&dir, "a", "2");
        fs::write(&path, "c: 1\n").unwrap();

        assert_eq!(one(true, &state, REF), updated(&path, true));
    }

    #[test]
    fn deleted_or_broken_file_is_manual() {
        let dir = TempDir::new().unwrap();
        let (state, path) = applied(&dir, "a", "2");

        fs::write(&path, "- not a mapping\n").unwrap();
        assert_eq!(one(true, &state, REF), updated(&path, true));
        fs::remove_file(&path).unwrap();
        assert_eq!(one(true, &state, REF), updated(&path, true));
    }

    #[test]
    fn a_tool_with_any_existing_file_is_applied() {
        let dir = TempDir::new().unwrap();
        let existing = dir.path().join("existing.yml");
        fs::write(&existing, "").unwrap();
        let missing = dir.path().join("missing.yml");
        let mut two = port("a", "1", Some(vec![existing.to_str().unwrap().into()]));
        two.targets
            .push(target(vec![missing.to_str().unwrap().into()]));

        let ops = plan(&selection(&["a"], &[]), &[two], &State::default(), REF);

        assert_eq!(
            ops[0].action,
            Action::Apply {
                paths: vec![existing, missing]
            }
        );
    }

    #[test]
    fn one_unresolvable_file_makes_the_tool_unsupported() {
        let mut two = port("a", "1", Some(vec!["/c".into()]));
        two.targets
            .push(target(vec!["$VIMIFY_TEST_UNSET/c".into()]));

        let ops = plan(&selection(&["a"], &[]), &[two], &State::default(), REF);

        assert_eq!(ops[0].action, Action::Unsupported);
    }

    #[test]
    fn an_update_reaches_a_file_new_to_the_port() {
        let dir = TempDir::new().unwrap();
        let (state, path) = applied(&dir, "a", "1");
        let added = dir.path().join("added.yml");
        let mut two = port("a", "2", Some(vec![path.to_str().unwrap().into()]));
        two.targets
            .push(target(vec![added.to_str().unwrap().into()]));

        let ops = plan(&selection(&["a"], &[]), &[two], &state, REF);

        assert_eq!(
            ops[0].action,
            Action::Update {
                paths: vec![path, added],
                manual: false
            }
        );
    }

    #[test]
    fn a_manual_edit_in_any_file_counts() {
        let dir = TempDir::new().unwrap();
        let (mut state, _) = applied(&dir, "a", "2");
        let other = dir.path().join("other.yml");
        fs::write(&other, "a:\n  b: x\n").unwrap();
        let mut second = state.tools["a"].files[0].clone();
        second.path = other.clone();
        second.file_hash = "written".into();
        state.tools.get_mut("a").unwrap().files.push(second);

        let ops = plan(&selection(&[], &["a"]), &[], &state, REF);

        assert!(ops[0].action.manual());
        assert_eq!(ops[0].action.paths()[1], other);
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
