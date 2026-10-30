use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, SendError, Sender};
use std::thread;

use crate::paths;
use crate::port::Port;

/// Earlier dirs win when a binary exists in several.
pub const DEFAULT_DIRS: &[&str] = &[
    "/usr/local/bin",
    "/opt/homebrew/bin",
    "/home/linuxbrew/.linuxbrew/bin",
    "~/.local/bin",
    "~/.cargo/bin",
    "~/go/bin",
    "$XDG_DATA_HOME/mise/shims",
    "~/.nix-profile/bin",
    "/snap/bin",
    "/usr/bin",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detection {
    pub port: String,
    pub path: PathBuf,
}

#[derive(Debug)]
pub enum Event {
    Scanned { done: usize, dir: PathBuf },
    Found(Detection),
    Done,
}

/// Expands, drops missing dirs and dedups by canonical path, keeping order.
pub fn resolve_dirs<'a>(raw: impl IntoIterator<Item = &'a str>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    raw.into_iter()
        .filter_map(paths::expand)
        .filter(|p| p.is_dir())
        .filter(|p| seen.insert(p.canonicalize().unwrap_or_else(|_| p.clone())))
        .collect()
}

pub fn spawn(dirs: Vec<PathBuf>, ports: Vec<Port>) -> Receiver<Event> {
    let (tx, rx) = mpsc::channel();
    // Send fails only once the UI has quit.
    thread::spawn(move || scan(&dirs, &ports, &tx));
    rx
}

pub fn scan(dirs: &[PathBuf], ports: &[Port], tx: &Sender<Event>) -> Result<(), SendError<Event>> {
    let mut found = HashSet::new();
    for (i, dir) in dirs.iter().enumerate() {
        // Unreadable dirs still count towards progress.
        let names: HashSet<OsString> = fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        for port in ports {
            if found.contains(&port.name) {
                continue;
            }
            if let Some(bin) = port
                .binaries
                .iter()
                .find(|b| names.contains(OsStr::new(b.as_str())))
            {
                found.insert(port.name.clone());
                tx.send(Event::Found(Detection {
                    port: port.name.clone(),
                    path: dir.join(bin),
                }))?;
            }
        }
        tx.send(Event::Scanned {
            done: i + 1,
            dir: dir.clone(),
        })?;
    }
    tx.send(Event::Done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn port(name: &str, binaries: &[&str]) -> Port {
        Port {
            name: name.into(),
            version: "0".into(),
            binaries: binaries.iter().map(|b| b.to_string()).collect(),
            target: None,
            keys: Default::default(),
            theme: Default::default(),
        }
    }

    fn dir_with(files: &[&str]) -> TempDir {
        let dir = TempDir::new().unwrap();
        for f in files {
            fs::write(dir.path().join(f), "").unwrap();
        }
        dir
    }

    fn run(dirs: &[PathBuf], ports: &[Port]) -> Vec<Event> {
        let (tx, rx) = mpsc::channel();
        scan(dirs, ports, &tx).unwrap();
        drop(tx);
        rx.into_iter().collect()
    }

    fn detections(events: &[Event]) -> Vec<Detection> {
        events
            .iter()
            .filter_map(|e| match e {
                Event::Found(d) => Some(d.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn detects_known_binaries_only() {
        let a = dir_with(&["lazygit", "unknown-tool"]);
        let events = run(&[a.path().into()], &[port("lazygit", &["lazygit"])]);
        assert_eq!(
            detections(&events),
            [Detection {
                port: "lazygit".into(),
                path: a.path().join("lazygit"),
            }]
        );
    }

    #[test]
    fn first_dir_wins() {
        let a = dir_with(&["lazygit"]);
        let b = dir_with(&["lazygit"]);
        let events = run(
            &[a.path().into(), b.path().into()],
            &[port("lazygit", &["lazygit"])],
        );
        let found = detections(&events);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, a.path().join("lazygit"));
    }

    #[test]
    fn any_listed_binary_matches() {
        let a = dir_with(&["nvim"]);
        let events = run(&[a.path().into()], &[port("neovim", &["vim", "nvim"])]);
        assert_eq!(detections(&events)[0].path, a.path().join("nvim"));
    }

    #[test]
    fn progress_per_dir_then_done() {
        let a = dir_with(&[]);
        let missing = a.path().join("gone");
        let events = run(&[a.path().into(), missing], &[]);
        let done: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Event::Scanned { done, .. } => Some(*done),
                _ => None,
            })
            .collect();
        assert_eq!(done, [1, 2]);
        assert!(matches!(events.last(), Some(Event::Done)));
    }

    #[test]
    fn resolve_drops_missing_and_duplicates() {
        let a = dir_with(&[]);
        let path = a.path().to_str().unwrap();
        let missing = format!("{path}/gone");
        let dup = format!("{path}/.");
        assert_eq!(
            resolve_dirs([path, missing.as_str(), dup.as_str()]),
            [PathBuf::from(path)]
        );
    }
}
