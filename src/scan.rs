use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, SendError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use semver::Version;

use crate::paths;
use crate::port::Port;

const VERSION_TIMEOUT: Duration = Duration::from_secs(2);

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
    pub version: Option<Version>,
}

#[derive(Debug)]
pub enum Event {
    Ports(Vec<Port>),
    Scanned { done: usize, dir: PathBuf },
    Found(Detection),
    Version { port: String, version: Version },
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

/// `refresh` updates the ports first: it decides which binaries the scan looks for.
pub fn spawn(
    dirs: Vec<PathBuf>,
    ports: Vec<Port>,
    refresh: fn(Vec<Port>) -> Vec<Port>,
) -> Receiver<Event> {
    let (tx, rx) = mpsc::channel();
    // Send fails only once the UI has quit.
    thread::spawn(move || {
        let ports = refresh(ports);
        tx.send(Event::Ports(ports.clone()))?;
        scan(&dirs, &ports, &tx)
    });
    rx
}

pub fn scan(dirs: &[PathBuf], ports: &[Port], tx: &Sender<Event>) -> Result<(), SendError<Event>> {
    let mut found = HashSet::new();
    let mut detected = Vec::new();
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
                let detection = Detection {
                    port: port.name.clone(),
                    path: dir.join(bin),
                    version: None,
                };
                detected.push((detection.clone(), port.version_args.clone()));
                tx.send(Event::Found(detection))?;
            }
        }
        tx.send(Event::Scanned {
            done: i + 1,
            dir: dir.clone(),
        })?;
    }
    // In parallel: one tool hanging on its version must not add up.
    let versions: Vec<_> = thread::scope(|s| {
        let threads: Vec<_> = detected
            .iter()
            .map(|(d, args)| s.spawn(move || (d.port.clone(), tool_version(&d.path, args))))
            .collect();
        threads.into_iter().filter_map(|t| t.join().ok()).collect()
    });
    for (port, version) in versions {
        if let Some(version) = version {
            tx.send(Event::Version { port, version })?;
        }
    }
    tx.send(Event::Done)
}

/// Runs the tool, which is why only ports' own binaries get here. `None` if it does not
/// answer: a tool that opens its UI instead must not hold up the scan.
fn tool_version(path: &Path, args: &[String]) -> Option<Version> {
    let mut child = Command::new(path)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let start = Instant::now();
    while child.try_wait().ok()?.is_none() {
        if start.elapsed() > VERSION_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        thread::sleep(Duration::from_millis(5));
    }
    let output = child.wait_with_output().ok()?;
    let text = [output.stdout, output.stderr].concat();
    parse_version(&String::from_utf8_lossy(&text))
}

/// The first dotted number, which holds for every tool's `--version` so far. Tools do
/// not all use semver, hence no strict parse: `2024.01` has a leading zero.
fn parse_version(text: &str) -> Option<Version> {
    let mut rest = text;
    while let Some(start) = rest.find(|c: char| c.is_ascii_digit()) {
        let number = &rest[start..];
        let end = number
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(number.len());
        let token = number[..end].trim_end_matches('.');
        let parts: Vec<&str> = token.split('.').collect();
        let glued = rest[..start].ends_with(|c: char| c.is_ascii_alphabetic() && c != 'v');
        if parts.len() >= 2 && !glued {
            let part = |i: usize| parts.get(i).and_then(|p| p.parse::<u64>().ok());
            return Some(Version::new(part(0)?, part(1)?, part(2).unwrap_or(0)));
        }
        rest = &number[end..];
        rest = rest.trim_start_matches(|c: char| c.is_ascii_alphanumeric());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use semver::VersionReq;
    use tempfile::TempDir;

    fn parsed(text: &str) -> Option<String> {
        parse_version(text).map(|v| v.to_string())
    }

    /// `sh -c`, not a script file: a file just written can fail with "Text file busy"
    /// while another test forks.
    fn shell(body: &str) -> Option<Version> {
        tool_version(Path::new("/bin/sh"), &["-c".to_owned(), body.to_owned()])
    }

    #[test]
    fn parses_real_version_output() {
        let cases = [
            (
                "commit=b741725a2413fc9cbc01529cab0e7cc654ad2054, build date=2026-10-11T09:23:32Z, build source=binaryRelease, version=0.66.1, os=linux, arch=amd64, git version=2.47.3",
                "0.66.1",
            ),
            ("Version: 0.25.2\nDate: 2026-04-19T02:51:21Z", "0.25.2"),
            ("LazySQL version:  0.7.0", "0.7.0"),
            (
                "lfk 0.19.4 (commit: 46376837, built: 2026-10-14T04:44:35Z)",
                "0.19.4",
            ),
            ("bat 0.25.0", "0.25.0"),
            ("btop version: 1.3.2", "1.3.2"),
            ("gitui 0.28.1", "0.28.1"),
            ("Yazi\n    Version: 26.9.1 (8dd895c 2026-09-01)", "26.9.1"),
            ("Version              v0.51.0", "0.51.0"),
        ];
        for (text, want) in cases {
            assert_eq!(parsed(text).as_deref(), Some(want), "{text}");
        }
    }

    #[test]
    fn pads_short_versions_and_allows_leading_zeros() {
        assert_eq!(parsed("tool 1.2").as_deref(), Some("1.2.0"));
        assert_eq!(parsed("tool 2024.01.05").as_deref(), Some("2024.1.5"));
    }

    #[test]
    fn text_without_a_version_is_none() {
        assert_eq!(parsed("Error: unknown flag: --version"), None);
        assert_eq!(parsed("build 12345 x86.64"), None);
        assert_eq!(parsed(""), None);
    }

    #[test]
    fn reads_the_version_a_tool_prints() {
        assert_eq!(shell("echo tool 1.2.3"), Some(Version::new(1, 2, 3)));
    }

    #[test]
    fn reads_a_version_from_stderr() {
        assert_eq!(shell("echo tool 1.2.3 >&2"), Some(Version::new(1, 2, 3)));
    }

    #[test]
    fn a_tool_that_hangs_gives_up() {
        let start = Instant::now();

        assert_eq!(shell("exec sleep 30"), None);

        assert!(start.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn a_tool_that_cannot_run_has_no_version() {
        assert_eq!(tool_version(Path::new("/nonexistent"), &[]), None);
    }

    fn port(name: &str, binaries: &[&str]) -> Port {
        Port {
            name: name.into(),
            version: "0".into(),
            binaries: binaries.iter().map(|b| b.to_string()).collect(),
            targets: Vec::new(),
            versions: VersionReq::STAR,
            version_args: Vec::new(),
            notes: None,
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
                version: None,
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
