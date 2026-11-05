use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{fs, io};

use anyhow::{Context as _, Result, ensure};

use crate::edit::Value;
use crate::palette::Palette;
use crate::plan::{Action, Op};
use crate::port::{Port, Target};
use crate::reference::Reference;
use crate::state::{self, Applied, AppliedFile, State};
use crate::{backup, edit, fsx, render};

pub struct Context<'a> {
    pub ports: &'a [Port],
    pub reference: &'a Reference,
    pub palette: &'a Palette,
    pub reference_hash: &'a str,
}

pub struct Report<'a> {
    pub op: &'a Op,
    /// Warnings on success.
    pub result: Result<Vec<String>>,
}

/// Each tool stands alone: a failure is reported and the others still run.
pub fn execute<'a>(
    ops: &'a [Op],
    cx: &Context,
    state: &mut State,
    persist: &mut dyn FnMut(&State) -> Result<()>,
) -> Vec<Report<'a>> {
    ops.iter()
        .filter(|op| op.action.writes())
        .map(|op| {
            let result = match &op.action {
                Action::Apply { paths }
                | Action::Create { paths }
                | Action::Update { paths, .. } => apply_tool(&op.tool, paths, cx, state, persist),
                Action::Revert { .. } => revert_tool(&op.tool, state, persist),
                Action::Current { .. } | Action::Unsupported => Ok(Vec::new()),
            };
            Report { op, result }
        })
        .collect()
}

pub struct Output {
    pub text: String,
    pub owned: BTreeMap<String, Value>,
    pub warnings: Vec<String>,
}

pub fn transform(
    tool: &str,
    target: &Target,
    reference: &Reference,
    palette: &Palette,
    text: &str,
) -> Result<Output> {
    let rendered = render::render(tool, target, reference, palette)?;
    let mut editor = edit::open(target.format, target.strategy, text)?;
    let mut owned = BTreeMap::new();
    for (setting, value) in rendered.values {
        editor.set(&split(&setting), value.clone())?;
        owned.insert(setting, value);
    }
    Ok(Output {
        text: editor.render(),
        owned,
        warnings: rendered.warnings,
    })
}

fn split(setting: &str) -> Vec<&str> {
    setting.split('.').collect()
}

/// One file of an apply, computed before any file is touched.
struct Pending<'a> {
    path: &'a Path,
    existing: Option<String>,
    target: &'a Target,
    output: Output,
}

/// All of a tool's files or none: a tool half configured may not even start.
fn apply_tool(
    tool: &str,
    paths: &[PathBuf],
    cx: &Context,
    state: &mut State,
    persist: &mut dyn FnMut(&State) -> Result<()>,
) -> Result<Vec<String>> {
    let port = cx
        .ports
        .iter()
        .find(|p| p.name == tool)
        .with_context(|| format!("no port for {tool}"))?;
    ensure!(
        !paths.is_empty() && port.targets.len() == paths.len(),
        "port has no target for each file of {tool}"
    );
    let pending = port
        .targets
        .iter()
        .zip(paths)
        .map(|(target, path)| {
            let existing = read(path)?;
            let original = existing.as_deref().unwrap_or_default();
            let output = transform(tool, target, cx.reference, cx.palette, original)
                .with_context(|| format!("{}", path.display()))?;
            Ok(Pending {
                path,
                existing,
                target,
                output,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let previous = state.tools.get(tool).cloned();
    let mut files = Vec::new();
    let mut warnings = Vec::new();
    for file in &pending {
        // A first apply always backs up: state with no backup means vimify created the
        // file, and revert would delete it.
        let changed = file.existing.as_deref() != Some(&file.output.text);
        let known = |a: &Applied| a.files.iter().any(|f| f.path == file.path);
        let first_apply = !previous.as_ref().is_some_and(known);
        let backup = match file.existing {
            Some(_) if changed || first_apply => Some(backup::backup(file.path)?),
            _ => None,
        };
        files.push(AppliedFile {
            path: file.path.to_owned(),
            backup,
            file_hash: state::hash(file.output.text.as_bytes()),
            format: file.target.format,
            strategy: file.target.strategy,
            owned: file.output.owned.clone(),
        });
        warnings.extend(file.output.warnings.iter().cloned());
    }

    state.record(
        tool,
        Applied {
            port_version: port.version.clone(),
            reference_hash: cx.reference_hash.to_owned(),
            files,
        },
    );
    // State first: a crash before the write leaves it pointing at a real backup. The
    // other order orphans the backup, and the next run backs up vimify's own output.
    let written = persist(state).and_then(|()| write_all(&pending));
    if let Err(e) = written {
        match previous {
            Some(applied) => state.tools.insert(tool.to_owned(), applied),
            None => state.tools.remove(tool),
        };
        let _ = persist(state);
        return Err(e);
    }
    Ok(warnings)
}

/// A failure puts back the files already written, so state and disk stay in step.
fn write_all(pending: &[Pending]) -> Result<()> {
    for (done, file) in pending.iter().enumerate() {
        let written = fsx::write_atomic(file.path, &file.output.text)
            .with_context(|| format!("{}", file.path.display()));
        if written.is_err() {
            for file in &pending[..done] {
                let _ = match &file.existing {
                    Some(text) => fsx::write_atomic(file.path, text),
                    None => fs::remove_file(file.path),
                };
            }
            return written;
        }
    }
    Ok(())
}

/// One file of a revert, computed before any file is touched.
struct Restore<'a> {
    path: &'a Path,
    exists: bool,
    /// `None` deletes the file.
    text: Option<String>,
    /// Mode of the backup, for a byte-for-byte restore.
    permissions: Option<fs::Permissions>,
}

/// Takes vimify's keys back out. The current files are backed up first, so nothing the
/// revert overrides is lost.
fn revert_tool(
    tool: &str,
    state: &mut State,
    persist: &mut dyn FnMut(&State) -> Result<()>,
) -> Result<Vec<String>> {
    let applied = state
        .tools
        .get(tool)
        .with_context(|| format!("{tool} is not applied"))?;
    // Prepared first: a lost backup or a broken file must leave every file alone.
    let restores = applied
        .files
        .iter()
        .map(prepare_restore)
        .collect::<Result<Vec<_>>>()?;

    for restore in &restores {
        if restore.exists {
            backup::backup(restore.path)?;
        }
        let path = restore.path;
        match &restore.text {
            Some(text) => {
                fsx::write_atomic(path, text).with_context(|| format!("{}", path.display()))?;
                if let Some(permissions) = &restore.permissions {
                    fs::set_permissions(path, permissions.clone())
                        .with_context(|| format!("{}", path.display()))?;
                }
            }
            None => match fs::remove_file(path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => {
                    return Err(e).with_context(|| format!("{}", path.display()));
                }
                _ => {}
            },
        }
    }
    // State last: after a crash the tool still counts as applied, and a retry restores
    // the same files.
    state.tools.remove(tool);
    persist(state)?;
    Ok(Vec::new())
}

fn prepare_restore(file: &AppliedFile) -> Result<Restore<'_>> {
    let path = file.path.as_path();
    let original = match &file.backup {
        Some(backup) => {
            let text = fs::read_to_string(backup)
                .with_context(|| format!("backup {}", backup.display()))?;
            let meta =
                fs::metadata(backup).with_context(|| format!("backup {}", backup.display()))?;
            Some((text, meta.permissions()))
        }
        None => None,
    };
    let current = read(path)?;
    // Only an edited file needs key-by-key: the backup restores anything else byte for
    // byte, which rewriting keys cannot promise.
    let edited = current
        .as_ref()
        .filter(|text| state::hash(text.as_bytes()) != file.file_hash);
    let (text, permissions) = match (edited, original) {
        (Some(text), original) => {
            let source = original.as_ref().map_or("", |(text, _)| text);
            let text =
                restore_keys(file, text, source).with_context(|| format!("{}", path.display()))?;
            (Some(text), None)
        }
        (None, Some((text, permissions))) => (Some(text), Some(permissions)),
        (None, None) => (None, None),
    };
    Ok(Restore {
        path,
        exists: current.is_some(),
        text,
        permissions,
    })
}

fn restore_keys(file: &AppliedFile, text: &str, original: &str) -> Result<String> {
    let mut editor = edit::open(file.format, file.strategy, text)?;
    for setting in file.owned.keys() {
        editor.restore(&split(setting), original)?;
    }
    Ok(editor.render())
}

fn read(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("{}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{palette, port};
    use anyhow::anyhow;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use tempfile::TempDir;

    const PORT: &str = "schema = 1\nversion = \"1\"\nbinaries = [\"t\"]\n[[target]]\nformat = \"yaml\"\nstrategy = \"edit\"\nkey_notation = \"angle\"\npaths = []\n[target.keys]\n\"a.down\" = \"down\"\n";

    struct Fixture {
        ports: Vec<Port>,
        reference: Reference,
        palette: Palette,
        state: State,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                ports: vec![port::parse("t", PORT).unwrap()],
                reference: Reference::default(),
                palette: palette::load("catppuccin-frappe").unwrap(),
                state: State::default(),
            }
        }

        fn run(&mut self, action: Action, hash: &str) -> Result<Vec<String>> {
            self.run_with(action, hash, &mut |_| Ok(()))
        }

        fn run_with(
            &mut self,
            action: Action,
            hash: &str,
            persist: &mut dyn FnMut(&State) -> Result<()>,
        ) -> Result<Vec<String>> {
            let ops = [Op {
                tool: "t".into(),
                action,
            }];
            let cx = Context {
                ports: &self.ports,
                reference: &self.reference,
                palette: &self.palette,
                reference_hash: hash,
            };
            let mut reports = execute(&ops, &cx, &mut self.state, persist);
            reports.remove(0).result
        }
    }

    fn config(dir: &TempDir, text: &str) -> PathBuf {
        let path = dir.path().join("config.yml");
        fs::write(&path, text).unwrap();
        path
    }

    fn bak(path: &Path, suffix: &str) -> PathBuf {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        name.into()
    }

    #[test]
    fn apply_backs_up_writes_and_records() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir, "# mine\nb: 1\n");
        let mut fx = Fixture::new();

        fx.run(
            Action::Apply {
                paths: vec![path.clone()],
            },
            "ref",
        )
        .unwrap();

        let written = fs::read_to_string(&path).unwrap();
        assert_eq!(written, "# mine\nb: 1\na:\n  down: j\n");
        assert_eq!(
            fs::read_to_string(bak(&path, ".bak")).unwrap(),
            "# mine\nb: 1\n"
        );
        let applied = &fx.state.tools["t"];
        assert_eq!(applied.port_version, "1");
        assert_eq!(applied.reference_hash, "ref");
        let file = &applied.files[0];
        assert_eq!(file.backup, Some(bak(&path, ".bak")));
        assert_eq!(file.path, path);
        assert_eq!(file.file_hash, state::hash(written.as_bytes()));
    }

    #[test]
    fn create_needs_no_backup() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("sub/config.yml");
        let mut fx = Fixture::new();

        fx.run(
            Action::Create {
                paths: vec![path.clone()],
            },
            "ref",
        )
        .unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "a:\n  down: j\n");
        assert_eq!(fx.state.tools["t"].files[0].backup, None);
    }

    #[test]
    fn first_apply_to_identical_file_still_backs_up() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir, "a:\n  down: j\n");
        let mut fx = Fixture::new();

        fx.run(
            Action::Apply {
                paths: vec![path.clone()],
            },
            "ref",
        )
        .unwrap();

        assert_eq!(
            fx.state.tools["t"].files[0].backup,
            Some(bak(&path, ".bak"))
        );
        assert!(bak(&path, ".bak").exists());
    }

    #[test]
    fn update_keeps_first_backup_as_revert_target() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir, "b: 1\n");
        let mut fx = Fixture::new();
        fx.run(
            Action::Apply {
                paths: vec![path.clone()],
            },
            "ref",
        )
        .unwrap();

        fx.reference.keys.insert("down".into(), "n".into());
        fx.run(
            Action::Update {
                paths: vec![path.clone()],
                manual: false,
            },
            "new",
        )
        .unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "b: 1\na:\n  down: n\n");
        assert_eq!(
            fx.state.tools["t"].files[0].backup,
            Some(bak(&path, ".bak"))
        );
        assert_eq!(fx.state.tools["t"].reference_hash, "new");
        assert_eq!(fs::read_to_string(bak(&path, ".bak")).unwrap(), "b: 1\n");
        assert!(bak(&path, ".bak.1").exists());
    }

    #[test]
    fn unchanged_update_writes_no_new_backup() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir, "b: 1\n");
        let mut fx = Fixture::new();
        fx.run(
            Action::Apply {
                paths: vec![path.clone()],
            },
            "ref",
        )
        .unwrap();

        fx.run(
            Action::Update {
                paths: vec![path.clone()],
                manual: false,
            },
            "ref",
        )
        .unwrap();

        assert!(!bak(&path, ".bak.1").exists());
    }

    #[test]
    fn symlinked_config_is_written_through() {
        let dir = TempDir::new().unwrap();
        let real = dir.path().join("real.yml");
        let link = dir.path().join("link.yml");
        fs::write(&real, "b: 1\n").unwrap();
        symlink(&real, &link).unwrap();
        let mut fx = Fixture::new();

        fx.run(
            Action::Apply {
                paths: vec![link.clone()],
            },
            "ref",
        )
        .unwrap();

        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(&real).unwrap(), "b: 1\na:\n  down: j\n");
        assert_eq!(fs::read_to_string(bak(&link, ".bak")).unwrap(), "b: 1\n");
    }

    #[test]
    fn unparsable_file_is_left_untouched() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir, "- not\n- a mapping\n");
        let mut fx = Fixture::new();

        assert!(
            fx.run(
                Action::Apply {
                    paths: vec![path.clone()]
                },
                "ref"
            )
            .is_err()
        );

        assert_eq!(fs::read_to_string(&path).unwrap(), "- not\n- a mapping\n");
        assert!(!bak(&path, ".bak").exists());
        assert!(fx.state.tools.is_empty());
    }

    #[test]
    fn state_failure_leaves_config_untouched() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir, "b: 1\n");
        let mut fx = Fixture::new();

        let result = fx.run_with(
            Action::Apply {
                paths: vec![path.clone()],
            },
            "ref",
            &mut |_| Err(anyhow!("disk full")),
        );

        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "b: 1\n");
        assert!(fx.state.tools.is_empty());
    }

    #[test]
    fn state_is_saved_before_the_config_is_written() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir, "b: 1\n");
        let mut fx = Fixture::new();
        let probe = path.clone();

        fx.run_with(Action::Apply { paths: vec![path] }, "ref", &mut |state| {
            assert_eq!(fs::read_to_string(&probe).unwrap(), "b: 1\n");
            assert!(state.tools.contains_key("t"));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn write_failure_rolls_state_back() {
        let dir = TempDir::new().unwrap();
        let locked = dir.path().join("locked");
        fs::create_dir(&locked).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
        let mut fx = Fixture::new();
        let mut saved = Vec::new();

        let result = fx.run_with(
            Action::Create {
                paths: vec![locked.join("config.yml")],
            },
            "ref",
            &mut |state| {
                saved.push(state.tools.len());
                Ok(())
            },
        );

        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(result.is_err());
        assert!(fx.state.tools.is_empty());
        assert_eq!(saved, [1, 0]);
    }

    #[test]
    fn one_failure_does_not_stop_the_others() {
        let dir = TempDir::new().unwrap();
        let bad = config(&dir, "- list\n");
        let good = dir.path().join("good.yml");
        let fx = Fixture::new();
        let mut state = State::default();
        let ops = [
            Op {
                tool: "t".into(),
                action: Action::Apply { paths: vec![bad] },
            },
            Op {
                tool: "t".into(),
                action: Action::Create {
                    paths: vec![good.clone()],
                },
            },
        ];
        let cx = Context {
            ports: &fx.ports,
            reference: &fx.reference,
            palette: &fx.palette,
            reference_hash: "ref",
        };

        let reports = execute(&ops, &cx, &mut state, &mut |_| Ok(()));

        assert!(reports[0].result.is_err());
        assert!(reports[1].result.is_ok());
        assert!(good.exists());
    }

    fn revert(path: &Path) -> Action {
        Action::Revert {
            paths: vec![path.to_owned()],
            manual: false,
        }
    }

    #[test]
    fn revert_restores_the_original_and_forgets_the_tool() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir, "# mine\nb: 1\n");
        let mut fx = Fixture::new();
        fx.run(
            Action::Apply {
                paths: vec![path.clone()],
            },
            "ref",
        )
        .unwrap();

        fx.run(revert(&path), "ref").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "# mine\nb: 1\n");
        assert!(fx.state.tools.is_empty());
        assert!(bak(&path, ".bak").exists());
    }

    #[test]
    fn revert_after_update_reaches_the_pre_vimify_file() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir, "b: 1\n");
        let mut fx = Fixture::new();
        fx.run(
            Action::Apply {
                paths: vec![path.clone()],
            },
            "ref",
        )
        .unwrap();
        fx.reference.keys.insert("down".into(), "n".into());
        fx.run(
            Action::Update {
                paths: vec![path.clone()],
                manual: false,
            },
            "new",
        )
        .unwrap();

        fx.run(revert(&path), "new").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "b: 1\n");
    }

    #[test]
    fn revert_keeps_edits_to_unowned_keys() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir, "# mine\nb: 1\n");
        let mut fx = Fixture::new();
        fx.run(
            Action::Apply {
                paths: vec![path.clone()],
            },
            "ref",
        )
        .unwrap();
        let applied = fs::read_to_string(&path).unwrap();
        fs::write(&path, applied.replace("b: 1", "b: 2 # edited")).unwrap();

        fx.run(revert(&path), "ref").unwrap();

        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "# mine\nb: 2 # edited\n"
        );
        assert!(fx.state.tools.is_empty());
    }

    #[test]
    fn revert_keeps_settings_added_under_an_owned_parent() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir, "b: 1\n");
        let mut fx = Fixture::new();
        fx.run(
            Action::Apply {
                paths: vec![path.clone()],
            },
            "ref",
        )
        .unwrap();
        fs::write(&path, "b: 1\na:\n  down: j\n  mine: 2\n").unwrap();

        fx.run(revert(&path), "ref").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "b: 1\na:\n  mine: 2\n");
    }

    #[test]
    fn revert_restores_a_value_vimify_replaced() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir, "a:\n  down: n # mine\nb: 1\n");
        let mut fx = Fixture::new();
        fx.run(
            Action::Apply {
                paths: vec![path.clone()],
            },
            "ref",
        )
        .unwrap();
        fs::write(&path, "a:\n  down: j\nb: 2\n").unwrap();

        fx.run(revert(&path), "ref").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "a:\n  down: n\nb: 2\n");
    }

    #[test]
    fn forced_revert_overrides_a_manual_key_and_keeps_a_copy() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir, "b: 1\n");
        let mut fx = Fixture::new();
        fx.run(
            Action::Apply {
                paths: vec![path.clone()],
            },
            "ref",
        )
        .unwrap();
        fs::write(&path, "b: 2\na:\n  down: x\n").unwrap();

        fx.run(revert(&path), "ref").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "b: 2\n");
        assert_eq!(
            fs::read_to_string(bak(&path, ".bak.1")).unwrap(),
            "b: 2\na:\n  down: x\n"
        );
    }

    #[test]
    fn revert_of_an_edited_created_file_keeps_the_edits() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yml");
        let mut fx = Fixture::new();
        fx.run(
            Action::Create {
                paths: vec![path.clone()],
            },
            "ref",
        )
        .unwrap();
        fs::write(&path, "a:\n  down: j\nmine: 1\n").unwrap();

        fx.run(revert(&path), "ref").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "mine: 1\n");
    }

    #[test]
    fn revert_of_a_broken_file_changes_nothing() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir, "b: 1\n");
        let mut fx = Fixture::new();
        fx.run(
            Action::Apply {
                paths: vec![path.clone()],
            },
            "ref",
        )
        .unwrap();
        fs::write(&path, "- not a mapping\n").unwrap();

        assert!(fx.run(revert(&path), "ref").is_err());

        assert_eq!(fs::read_to_string(&path).unwrap(), "- not a mapping\n");
        assert!(fx.state.tools.contains_key("t"));
        assert!(!bak(&path, ".bak.1").exists());
    }

    #[test]
    fn revert_cleans_up_a_key_the_port_no_longer_writes() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir, "b: 1\n");
        let mut fx = Fixture::new();
        fx.run(
            Action::Apply {
                paths: vec![path.clone()],
            },
            "ref",
        )
        .unwrap();
        fx.ports[0] = port::parse("t", &PORT.replace("a.down", "a.up")).unwrap();
        fx.run(
            Action::Update {
                paths: vec![path.clone()],
                manual: false,
            },
            "ref",
        )
        .unwrap();
        fs::write(
            &path,
            fs::read_to_string(&path).unwrap().replace("b: 1", "b: 2"),
        )
        .unwrap();

        fx.run(revert(&path), "ref").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "b: 2\n");
    }

    #[test]
    fn revert_deletes_a_created_file_and_keeps_a_copy() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yml");
        let mut fx = Fixture::new();
        fx.run(
            Action::Create {
                paths: vec![path.clone()],
            },
            "ref",
        )
        .unwrap();

        fx.run(revert(&path), "ref").unwrap();

        assert!(!path.exists());
        assert_eq!(
            fs::read_to_string(bak(&path, ".bak")).unwrap(),
            "a:\n  down: j\n"
        );
        assert!(fx.state.tools.is_empty());
    }

    #[test]
    fn revert_of_a_created_file_already_gone_just_forgets_it() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.yml");
        let mut fx = Fixture::new();
        fx.run(
            Action::Create {
                paths: vec![path.clone()],
            },
            "ref",
        )
        .unwrap();
        fs::remove_file(&path).unwrap();

        fx.run(revert(&path), "ref").unwrap();

        assert!(fx.state.tools.is_empty());
        assert!(!bak(&path, ".bak").exists());
    }

    #[test]
    fn revert_recreates_a_deleted_file_with_its_mode() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir, "b: 1\n");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        let mut fx = Fixture::new();
        fx.run(
            Action::Apply {
                paths: vec![path.clone()],
            },
            "ref",
        )
        .unwrap();
        fs::remove_file(&path).unwrap();

        fx.run(revert(&path), "ref").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "b: 1\n");
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640);
    }

    #[test]
    fn revert_writes_through_a_symlink() {
        let dir = TempDir::new().unwrap();
        let real = dir.path().join("real.yml");
        let link = dir.path().join("link.yml");
        fs::write(&real, "b: 1\n").unwrap();
        symlink(&real, &link).unwrap();
        let mut fx = Fixture::new();
        fx.run(
            Action::Apply {
                paths: vec![link.clone()],
            },
            "ref",
        )
        .unwrap();

        fx.run(revert(&link), "ref").unwrap();

        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(&real).unwrap(), "b: 1\n");
    }

    #[test]
    fn lost_backup_leaves_file_and_state_alone() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir, "b: 1\n");
        let mut fx = Fixture::new();
        fx.run(
            Action::Apply {
                paths: vec![path.clone()],
            },
            "ref",
        )
        .unwrap();
        let applied = fs::read_to_string(&path).unwrap();
        fs::remove_file(bak(&path, ".bak")).unwrap();

        assert!(fx.run(revert(&path), "ref").is_err());

        assert_eq!(fs::read_to_string(&path).unwrap(), applied);
        assert!(fx.state.tools.contains_key("t"));
        assert!(!bak(&path, ".bak").exists());
    }

    #[test]
    fn state_is_saved_after_the_file_is_restored() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir, "b: 1\n");
        let mut fx = Fixture::new();
        fx.run(
            Action::Apply {
                paths: vec![path.clone()],
            },
            "ref",
        )
        .unwrap();
        let probe = path.clone();

        fx.run_with(revert(&path), "ref", &mut |state| {
            assert_eq!(fs::read_to_string(&probe).unwrap(), "b: 1\n");
            assert!(state.tools.is_empty());
            Ok(())
        })
        .unwrap();
    }

    const TWO_FILES: &str = "schema = 1\nversion = \"1\"\nbinaries = [\"t\"]\n[[target]]\nformat = \"yaml\"\nstrategy = \"edit\"\nkey_notation = \"angle\"\npaths = []\n[target.keys]\n\"a.down\" = \"down\"\n[[target]]\nformat = \"toml\"\nstrategy = \"edit\"\npaths = []\n[target.theme]\n\"c.d\" = \"{accent}\"\n";

    fn two_files(dir: &TempDir) -> (Fixture, PathBuf, PathBuf) {
        let mut fx = Fixture::new();
        fx.ports = vec![port::parse("t", TWO_FILES).unwrap()];
        let yaml = dir.path().join("a.yml");
        let toml = dir.path().join("b.toml");
        (fx, yaml, toml)
    }

    #[test]
    fn each_file_gets_its_own_settings_and_backup() {
        let dir = TempDir::new().unwrap();
        let (mut fx, yaml, toml) = two_files(&dir);
        fs::write(&yaml, "b: 1\n").unwrap();
        let paths = vec![yaml.clone(), toml.clone()];

        fx.run(Action::Apply { paths }, "ref").unwrap();

        assert_eq!(fs::read_to_string(&yaml).unwrap(), "b: 1\na:\n  down: j\n");
        assert_eq!(fs::read_to_string(&toml).unwrap(), "[c]\nd = \"#8caaee\"\n");
        let files = &fx.state.tools["t"].files;
        assert_eq!(files[0].backup, Some(bak(&yaml, ".bak")));
        assert_eq!(files[1].backup, None);
        assert_eq!(files[1].format, port::Format::Toml);
    }

    #[test]
    fn revert_undoes_every_file() {
        let dir = TempDir::new().unwrap();
        let (mut fx, yaml, toml) = two_files(&dir);
        fs::write(&yaml, "b: 1\n").unwrap();
        let paths = vec![yaml.clone(), toml.clone()];
        fx.run(Action::Apply { paths }, "ref").unwrap();

        fx.run(revert(&yaml), "ref").unwrap();

        assert_eq!(fs::read_to_string(&yaml).unwrap(), "b: 1\n");
        assert!(!toml.exists());
        assert!(fx.state.tools.is_empty());
    }

    #[test]
    fn a_broken_second_file_stops_the_tool_before_any_write() {
        let dir = TempDir::new().unwrap();
        let (mut fx, yaml, toml) = two_files(&dir);
        fs::write(&yaml, "b: 1\n").unwrap();
        fs::write(&toml, "c = [").unwrap();
        let paths = vec![yaml.clone(), toml.clone()];

        assert!(fx.run(Action::Apply { paths }, "ref").is_err());

        assert_eq!(fs::read_to_string(&yaml).unwrap(), "b: 1\n");
        assert!(!bak(&yaml, ".bak").exists());
        assert!(fx.state.tools.is_empty());
    }

    #[test]
    fn a_failed_second_write_puts_the_first_file_back() {
        let dir = TempDir::new().unwrap();
        let (mut fx, yaml, _) = two_files(&dir);
        let created = dir.path().join("created.yml");
        let locked = dir.path().join("locked");
        fs::create_dir(&locked).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
        fs::write(&yaml, "b: 1\n").unwrap();
        let failed = locked.join("b.toml");

        let existing = fx.run(
            Action::Apply {
                paths: vec![yaml.clone(), failed.clone()],
            },
            "ref",
        );
        let missing = fx.run(
            Action::Create {
                paths: vec![created.clone(), failed],
            },
            "ref",
        );

        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(existing.is_err() && missing.is_err());
        assert_eq!(fs::read_to_string(&yaml).unwrap(), "b: 1\n");
        assert!(!created.exists());
        assert!(fx.state.tools.is_empty());
    }

    #[test]
    fn a_lost_backup_of_one_file_leaves_every_file_alone() {
        let dir = TempDir::new().unwrap();
        let (mut fx, yaml, toml) = two_files(&dir);
        fs::write(&yaml, "b: 1\n").unwrap();
        fs::write(&toml, "e = 1\n").unwrap();
        let paths = vec![yaml.clone(), toml.clone()];
        fx.run(Action::Apply { paths }, "ref").unwrap();
        let first = fs::read_to_string(&yaml).unwrap();
        fs::remove_file(bak(&toml, ".bak")).unwrap();

        assert!(fx.run(revert(&yaml), "ref").is_err());

        assert_eq!(fs::read_to_string(&yaml).unwrap(), first);
        assert!(!bak(&yaml, ".bak.1").exists());
        assert!(fx.state.tools.contains_key("t"));
    }

    #[test]
    fn revert_of_an_edited_block_keeps_the_user_lines() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir, "--paging=never\n");
        let text = "schema = 1\nversion = \"1\"\nbinaries = [\"t\"]\n[[target]]\nformat = \"lines\"\nstrategy = \"block\"\npaths = []\ntemplate = \"--theme=x\"\n";
        let mut fx = Fixture::new();
        fx.ports = vec![port::parse("t", text).unwrap()];
        fx.run(
            Action::Apply {
                paths: vec![path.clone()],
            },
            "ref",
        )
        .unwrap();
        let applied = fs::read_to_string(&path).unwrap();
        fs::write(&path, format!("{applied}--wrap=never\n")).unwrap();

        fx.run(revert(&path), "ref").unwrap();

        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "--paging=never\n--wrap=never\n"
        );
    }

    #[test]
    fn ports_match_their_golden_files() {
        let reference = Reference::default();
        let palette = palette::load("catppuccin-frappe").unwrap();
        let cases = port::golden_cases();
        assert!(!cases.is_empty());
        for (port, target, input, expected) in cases {
            let target = &target;
            let name = &port.name;
            let out = transform(name, target, &reference, &palette, input).unwrap();
            assert_eq!(out.text, expected, "port {name}");
            assert!(out.warnings.is_empty(), "port {name}: {:?}", out.warnings);
            let again = transform(name, target, &reference, &palette, &out.text).unwrap();
            assert_eq!(again.text, out.text, "port {name} is not idempotent");

            let mut editor = edit::open(target.format, target.strategy, &out.text).unwrap();
            for setting in out.owned.keys() {
                editor.restore(&split(setting), input).unwrap();
            }
            assert_eq!(editor.render(), input, "port {name} does not restore");
        }
    }
}
