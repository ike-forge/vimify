use std::path::Path;
use std::{fs, io};

use anyhow::{Context as _, Result, anyhow};

use crate::palette::Palette;
use crate::plan::{Action, Op};
use crate::port::Port;
use crate::reference::Reference;
use crate::state::{self, Applied, State};
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
                Action::Apply { path } | Action::Create { path } | Action::Update { path } => {
                    apply_tool(&op.tool, path, cx, state, persist)
                }
                // TODO: restore the backup, or delete a file vimify created.
                Action::Revert { .. } => Err(anyhow!("revert is not implemented yet")),
                Action::Current { .. } | Action::Unsupported => Ok(Vec::new()),
            };
            Report { op, result }
        })
        .collect()
}

/// New file text for `text`, and any warnings.
pub fn transform(
    port: &Port,
    reference: &Reference,
    palette: &Palette,
    text: &str,
) -> Result<(String, Vec<String>)> {
    let target = port.target.as_ref().context("port has no target")?;
    let rendered = render::render(port, reference, palette)?;
    let mut editor = edit::open(target.format, text)?;
    for (setting, value) in rendered.values {
        let path: Vec<&str> = setting.split('.').collect();
        editor.set(&path, value)?;
    }
    Ok((editor.render(), rendered.warnings))
}

fn apply_tool(
    tool: &str,
    path: &Path,
    cx: &Context,
    state: &mut State,
    persist: &mut dyn FnMut(&State) -> Result<()>,
) -> Result<Vec<String>> {
    let port = cx
        .ports
        .iter()
        .find(|p| p.name == tool)
        .with_context(|| format!("no port for {tool}"))?;
    let existing = read(path)?;
    let original = existing.as_deref().unwrap_or_default();
    let (output, warnings) = transform(port, cx.reference, cx.palette, original)
        .with_context(|| format!("{}", path.display()))?;

    // A first apply always backs up: state with no backup means vimify created the
    // file, and revert would delete it.
    let changed = existing.as_deref() != Some(&output);
    let first_apply = !state.tools.contains_key(tool);
    let backup = match existing {
        Some(_) if changed || first_apply => Some(backup::backup(path)?),
        _ => None,
    };

    let previous = state.tools.get(tool).cloned();
    state.record(
        tool,
        Applied {
            port_version: port.version.clone(),
            path: path.to_owned(),
            backup,
            reference_hash: cx.reference_hash.to_owned(),
            file_hash: state::hash(output.as_bytes()),
        },
    );
    // State first: a crash before the write leaves it pointing at a real backup. The
    // other order orphans the backup, and the next run backs up vimify's own output.
    let written = persist(state).and_then(|()| {
        fsx::write_atomic(path, &output).with_context(|| format!("{}", path.display()))
    });
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
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::PathBuf;
    use tempfile::TempDir;

    const PORT: &str = "schema = 1\nversion = \"1\"\nbinaries = [\"t\"]\n[target]\nformat = \"yaml\"\nstrategy = \"edit\"\nkey_notation = \"angle\"\npaths = []\n[keys]\n\"a.down\" = \"down\"\n";

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

        fx.run(Action::Apply { path: path.clone() }, "ref").unwrap();

        let written = fs::read_to_string(&path).unwrap();
        assert_eq!(written, "# mine\nb: 1\na:\n  down: j\n");
        assert_eq!(
            fs::read_to_string(bak(&path, ".bak")).unwrap(),
            "# mine\nb: 1\n"
        );
        let applied = &fx.state.tools["t"];
        assert_eq!(applied.backup, Some(bak(&path, ".bak")));
        assert_eq!(applied.path, path);
        assert_eq!(applied.port_version, "1");
        assert_eq!(applied.reference_hash, "ref");
        assert_eq!(applied.file_hash, state::hash(written.as_bytes()));
    }

    #[test]
    fn create_needs_no_backup() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("sub/config.yml");
        let mut fx = Fixture::new();

        fx.run(Action::Create { path: path.clone() }, "ref")
            .unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "a:\n  down: j\n");
        assert_eq!(fx.state.tools["t"].backup, None);
    }

    #[test]
    fn first_apply_to_identical_file_still_backs_up() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir, "a:\n  down: j\n");
        let mut fx = Fixture::new();

        fx.run(Action::Apply { path: path.clone() }, "ref").unwrap();

        assert_eq!(fx.state.tools["t"].backup, Some(bak(&path, ".bak")));
        assert!(bak(&path, ".bak").exists());
    }

    #[test]
    fn update_keeps_first_backup_as_revert_target() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir, "b: 1\n");
        let mut fx = Fixture::new();
        fx.run(Action::Apply { path: path.clone() }, "ref").unwrap();

        fx.reference.keys.insert("down".into(), "n".into());
        fx.run(Action::Update { path: path.clone() }, "new")
            .unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "b: 1\na:\n  down: n\n");
        assert_eq!(fx.state.tools["t"].backup, Some(bak(&path, ".bak")));
        assert_eq!(fx.state.tools["t"].reference_hash, "new");
        assert_eq!(fs::read_to_string(bak(&path, ".bak")).unwrap(), "b: 1\n");
        assert!(bak(&path, ".bak.1").exists());
    }

    #[test]
    fn unchanged_update_writes_no_new_backup() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir, "b: 1\n");
        let mut fx = Fixture::new();
        fx.run(Action::Apply { path: path.clone() }, "ref").unwrap();

        fx.run(Action::Update { path: path.clone() }, "ref")
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

        fx.run(Action::Apply { path: link.clone() }, "ref").unwrap();

        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(&real).unwrap(), "b: 1\na:\n  down: j\n");
        assert_eq!(fs::read_to_string(bak(&link, ".bak")).unwrap(), "b: 1\n");
    }

    #[test]
    fn unparsable_file_is_left_untouched() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir, "- not\n- a mapping\n");
        let mut fx = Fixture::new();

        assert!(fx.run(Action::Apply { path: path.clone() }, "ref").is_err());

        assert_eq!(fs::read_to_string(&path).unwrap(), "- not\n- a mapping\n");
        assert!(!bak(&path, ".bak").exists());
        assert!(fx.state.tools.is_empty());
    }

    #[test]
    fn state_failure_leaves_config_untouched() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir, "b: 1\n");
        let mut fx = Fixture::new();

        let result = fx.run_with(Action::Apply { path: path.clone() }, "ref", &mut |_| {
            Err(anyhow!("disk full"))
        });

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

        fx.run_with(Action::Apply { path }, "ref", &mut |state| {
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
                path: locked.join("config.yml"),
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
                action: Action::Apply { path: bad },
            },
            Op {
                tool: "t".into(),
                action: Action::Create { path: good.clone() },
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

    #[test]
    fn revert_is_reported_as_not_done() {
        let dir = TempDir::new().unwrap();
        let path = config(&dir, "b: 1\n");
        let mut fx = Fixture::new();

        assert!(
            fx.run(Action::Revert { path: path.clone() }, "ref")
                .is_err()
        );
        assert_eq!(fs::read_to_string(path).unwrap(), "b: 1\n");
    }

    #[test]
    fn ports_match_their_golden_files() {
        let reference = Reference::default();
        let palette = palette::load("catppuccin-frappe").unwrap();
        let ports = port::embedded().unwrap();
        let cases = port::golden_cases();
        assert!(!cases.is_empty());
        for (name, input, expected) in cases {
            let port = ports.iter().find(|p| p.name == name).unwrap();
            let (out, warnings) = transform(port, &reference, &palette, input).unwrap();
            assert_eq!(out, expected, "port {name}");
            assert!(warnings.is_empty(), "port {name}: {warnings:?}");
            let (again, _) = transform(port, &reference, &palette, &out).unwrap();
            assert_eq!(again, out, "port {name} is not idempotent");
        }
    }
}
