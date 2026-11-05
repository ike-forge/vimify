use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use include_dir::{Dir, include_dir};
use semver::{Op, Version, VersionReq};
use serde::{Deserialize, Serialize};

use crate::actions::{self, Notation};
use crate::{edit, palette, paths};

static EMBEDDED: Dir = include_dir!("$CARGO_MANIFEST_DIR/ports");

pub const SCHEMA: u32 = 1;

#[derive(Debug, Clone)]
pub struct Port {
    pub name: String,
    pub version: String,
    pub binaries: Vec<String>,
    /// Tool versions this port was written for.
    pub versions: VersionReq,
    /// Arguments that print the tool's version.
    pub version_args: Vec<String>,
    /// One per file. Splitting a tool's config across files changes nothing else.
    pub targets: Vec<Target>,
    /// Why the port covers what it does; shown in the info modal.
    pub notes: Option<String>,
}

impl Port {
    pub fn writable(&self) -> bool {
        self.unsupported_reason().is_none()
    }

    pub fn unsupported_reason(&self) -> Option<&'static str> {
        if self.targets.is_empty() {
            return Some("vimify does not know this tool's config yet");
        }
        if !self.targets.iter().all(Target::editable) {
            return Some("vimify cannot edit this config format yet");
        }
        if self.targets.iter().any(Target::empty) {
            return Some("this port has no mappings yet");
        }
        None
    }

    /// Lowest version of the range, for ordering a tool's ports.
    fn floor(&self) -> Version {
        self.versions
            .comparators
            .iter()
            .find(|c| {
                matches!(
                    c.op,
                    Op::GreaterEq | Op::Greater | Op::Exact | Op::Caret | Op::Tilde
                )
            })
            .map_or(Version::new(0, 0, 0), |c| {
                Version::new(c.major, c.minor.unwrap_or(0), c.patch.unwrap_or(0))
            })
    }

    /// Actions this port binds and those it leaves alone, in vocabulary order.
    pub fn action_coverage(&self) -> (Vec<&'static str>, Vec<&'static str>) {
        actions::DEFAULTS
            .iter()
            .map(|(action, _)| *action)
            .partition(|action| {
                let bound = |t: &Target| t.keys.values().any(|a| a == action);
                self.targets.iter().any(bound)
            })
    }

    pub fn theme_settings(&self) -> usize {
        self.targets.iter().map(|t| t.theme.len()).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum ThemeValue {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Toml,
    Yaml,
    Json,
    Lines,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Strategy {
    Edit,
    Block,
    Include,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Target {
    pub format: Format,
    pub strategy: Strategy,
    pub key_notation: Option<Notation>,
    pub paths: Vec<String>,
    /// Dotted tool setting → action.
    #[serde(default)]
    pub keys: BTreeMap<String, String>,
    /// Dotted tool setting → text with `{role}` placeholders, or a list of them.
    #[serde(default)]
    pub theme: BTreeMap<String, ThemeValue>,
    /// Text of a `block` target, with `{role}` placeholders.
    pub template: Option<String>,
}

impl Target {
    /// First existing path, else the first that expands (to be created).
    /// The bool is whether the file exists.
    pub fn resolve(&self) -> Option<(PathBuf, bool)> {
        let candidates = self.candidates();
        if let Some(existing) = candidates.iter().find(|p| p.exists()) {
            return Some((existing.clone(), true));
        }
        candidates.into_iter().next().map(|p| (p, false))
    }

    pub fn candidates(&self) -> Vec<PathBuf> {
        self.paths.iter().filter_map(|p| paths::expand(p)).collect()
    }

    fn editable(&self) -> bool {
        edit::supports(self.format, self.strategy)
    }

    fn empty(&self) -> bool {
        self.keys.is_empty() && self.theme.is_empty() && self.template.is_none()
    }
}

#[derive(Deserialize)]
struct PortFile {
    schema: u32,
    version: String,
    binaries: Vec<String>,
    versions: Option<String>,
    #[serde(default = "default_version_args")]
    version_args: Vec<String>,
    #[serde(default)]
    target: Vec<Target>,
    notes: Option<String>,
}

fn default_version_args() -> Vec<String> {
    vec!["--version".to_owned()]
}

/// Ports compiled into the binary, sorted by name.
pub fn embedded() -> Result<Vec<Port>> {
    let mut ports = sources()
        .iter()
        .map(|(tool, text)| parse(tool, text))
        .collect::<Result<Vec<_>>>()?;
    ports.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(ports)
}

/// Tool name and text of every embedded port.
pub fn sources() -> Vec<(String, String)> {
    sources_in(&EMBEDDED)
}

/// `<tool>/port.toml`, or `<tool>/<anything>/port.toml` for a tool with several ranges.
fn sources_in(root: &Dir) -> Vec<(String, String)> {
    let mut sources = Vec::new();
    for tool in root.dirs() {
        let name = tool.path().to_string_lossy().into_owned();
        for dir in port_dirs(tool) {
            let text = dir
                .get_file(dir.path().join("port.toml"))
                .and_then(|f| f.contents_utf8());
            if let Some(text) = text {
                sources.push((name.clone(), text.to_owned()));
            }
        }
    }
    sources
}

fn port_dirs<'a>(tool: &'a Dir<'a>) -> Vec<&'a Dir<'a>> {
    std::iter::once(tool).chain(tool.dirs()).collect()
}

/// The port for this tool and version, and whether it is untested there: outside its
/// range and on another major version. The same major counts as supported, since ports
/// always lag releases and config formats rarely break within one.
pub fn resolve(ports: &[Port], tool: &str, version: Option<&Version>) -> Option<(Port, bool)> {
    let of_tool = || ports.iter().filter(|p| p.name == tool);
    let mut usable: Vec<&Port> = of_tool().filter(|p| p.writable()).collect();
    usable.sort_by_key(|p| p.floor());
    let Some(version) = version else {
        return match usable.last() {
            Some(port) => Some(((*port).clone(), true)),
            None => of_tool().next().map(|p| (p.clone(), false)),
        };
    };
    if let Some(port) = usable.iter().rev().find(|p| p.versions.matches(version)) {
        return Some(((*port).clone(), false));
    }
    let nearest = usable
        .iter()
        .rev()
        .find(|p| p.floor() <= *version)
        .or(usable.first());
    match nearest {
        Some(port) => Some(((*port).clone(), port.floor().major != version.major)),
        None => of_tool().next().map(|p| (p.clone(), false)),
    }
}

pub fn parse(name: &str, text: &str) -> Result<Port> {
    let file: PortFile = toml::from_str(text).with_context(|| format!("port {name}"))?;
    if file.schema != SCHEMA {
        bail!("port {name}: unsupported schema {}", file.schema);
    }
    let versions = file.versions.as_deref().unwrap_or("*");
    let port = Port {
        name: name.to_owned(),
        version: file.version,
        binaries: file.binaries,
        versions: versions
            .parse()
            .with_context(|| format!("port {name}: versions {versions}"))?,
        version_args: file.version_args,
        targets: file.target,
        notes: file.notes,
    };
    validate(&port).with_context(|| format!("port {name}"))?;
    Ok(port)
}

/// Variables that point outside a tool's config.
const FORBIDDEN_VARS: &[&str] = &[
    "HOME",
    "OLDPWD",
    "PWD",
    "TMPDIR",
    "XDG_CACHE_HOME",
    "XDG_DATA_HOME",
    "XDG_RUNTIME_DIR",
    "XDG_STATE_HOME",
];

/// Ports arrive over the network, so a port must not reach arbitrary files. This bounds
/// the damage; it does not replace reviewing what a port writes.
fn validate_path(path: &str) -> Result<()> {
    if path.split('/').any(|segment| segment == "..") {
        bail!("path {path} leaves its directory");
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if rest.starts_with(".config/") || rest.starts_with("Library/Application Support/") {
            return Ok(());
        }
        bail!("path {path} is outside the config directories");
    }
    let var = path
        .strip_prefix('$')
        .map(|p| p.split('/').next().unwrap_or(p));
    match var {
        Some(var) if !FORBIDDEN_VARS.contains(&var) => Ok(()),
        _ => bail!("path {path} is outside the config directories"),
    }
}

/// vimify runs these on the user's machine, so only plain words and flags pass.
fn validate_version_args(args: &[String]) -> Result<()> {
    let plain = |arg: &String| {
        !arg.is_empty() && arg.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    };
    if args.len() > 3 || !args.iter().all(plain) {
        bail!("version_args must be up to three plain words or flags");
    }
    Ok(())
}

/// Ports cannot define new actions or roles.
fn validate(port: &Port) -> Result<()> {
    validate_version_args(&port.version_args)?;
    port.targets.iter().try_for_each(validate_target)
}

fn validate_target(target: &Target) -> Result<()> {
    target.paths.iter().try_for_each(|p| validate_path(p))?;
    for action in target.keys.values() {
        if !actions::is_action(action) {
            bail!("unknown action {action}");
        }
    }
    if !target.keys.is_empty() && target.key_notation.is_none() {
        bail!("keys need a key_notation");
    }
    if target.template.is_some() != (target.strategy == Strategy::Block) {
        bail!("a template belongs to a block target, and a block target needs one");
    }
    let values = target.theme.values().flat_map(|value| match value {
        ThemeValue::One(s) => std::slice::from_ref(s),
        ThemeValue::Many(items) => items.as_slice(),
    });
    for text in values.chain(&target.template) {
        for role in palette::placeholders(text) {
            if !palette::ROLES.contains(&role) {
                bail!("unknown role {role}");
            }
        }
    }
    Ok(())
}

/// Target, input and expected text of every `tests/*.input.*` file. `keymap.input.toml`
/// tests the target writing `keymap.toml`; a stem no target has tests the first.
#[cfg(test)]
pub fn golden_cases() -> Vec<(Port, Target, &'static str, &'static str)> {
    let mut cases = Vec::new();
    for tool in EMBEDDED.dirs() {
        let name = tool.path().to_string_lossy().into_owned();
        for dir in port_dirs(tool) {
            let Some(tests) = dir.get_dir(dir.path().join("tests")) else {
                continue;
            };
            let text = dir
                .get_file(dir.path().join("port.toml"))
                .and_then(|f| f.contents_utf8())
                .unwrap();
            for file in tests.files() {
                let path = file.path().to_string_lossy();
                if let Some((stem, ext)) = path.rsplit_once(".input.") {
                    let expected = EMBEDDED
                        .get_file(format!("{stem}.expected.{ext}"))
                        .and_then(|f| f.contents_utf8())
                        .unwrap_or_else(|| panic!("{path} has no expected file"));
                    let port = parse(&name, text).unwrap();
                    let test = stem.rsplit('/').next().unwrap();
                    let writes = |t: &&Target| t.paths.iter().any(|p| file_stem(p) == Some(test));
                    let target = port.targets.iter().find(writes).unwrap_or(&port.targets[0]);
                    let target = target.clone();
                    cases.push((port, target, file.contents_utf8().unwrap(), expected));
                }
            }
        }
    }
    cases
}

#[cfg(test)]
fn file_stem(path: &str) -> Option<&str> {
    path.rsplit('/').next()?.split('.').next()
}

#[cfg(test)]
mod tests {
    use super::*;

    const YAML: &str = "schema = 1\nversion = \"1\"\nbinaries = []\n[[target]]\nformat = \"yaml\"\nstrategy = \"edit\"\nkey_notation = \"angle\"\npaths = []\n";

    static MULTI: Dir = include_dir!("$CARGO_MANIFEST_DIR/testdata/ports");

    fn ranged(versions: &str, version: &str) -> Port {
        let text = YAML
            .replace("version = \"1\"", &format!("version = \"{version}\""))
            .replace(
                "binaries = []",
                &format!("binaries = []\nversions = \"{versions}\""),
            );
        parse("t", &format!("{text}[target.keys]\na = \"up\"\n")).unwrap()
    }

    fn pick(ports: &[Port], version: &str) -> (String, bool) {
        let version = Version::parse(version).unwrap();
        let (port, untested) = resolve(ports, "t", Some(&version)).unwrap();
        (port.version, untested)
    }

    #[test]
    fn a_tool_can_have_several_ranges_on_disk() {
        let sources = sources_in(&MULTI);
        assert_eq!(sources.len(), 2);
        assert!(sources.iter().all(|(tool, _)| tool == "multi"));
    }

    #[test]
    fn resolve_picks_the_range_the_version_is_in() {
        let ports = [ranged("<1.0", "old"), ranged(">=1.0, <2.0", "new")];
        assert_eq!(pick(&ports, "0.5.0"), ("old".into(), false));
        assert_eq!(pick(&ports, "1.5.0"), ("new".into(), false));
    }

    #[test]
    fn a_newer_version_with_the_same_major_is_supported() {
        let ports = [ranged(">=1.0, <1.5", "a")];
        assert_eq!(pick(&ports, "1.9.0"), ("a".into(), false));
    }

    #[test]
    fn another_major_version_is_untested() {
        let ports = [ranged(">=1.0, <2.0", "a")];
        assert_eq!(pick(&ports, "2.0.0"), ("a".into(), true));
        assert_eq!(pick(&ports, "0.9.0"), ("a".into(), true));
    }

    #[test]
    fn past_every_range_the_newest_port_is_used() {
        let ports = [ranged(">=2.0, <3.0", "new"), ranged(">=1.0, <2.0", "old")];
        assert_eq!(pick(&ports, "3.1.0"), ("new".into(), true));
    }

    #[test]
    fn an_unknown_version_is_untested_on_the_newest_port() {
        let ports = [ranged("<1.0", "old"), ranged(">=1.0", "new")];
        let (port, untested) = resolve(&ports, "t", None).unwrap();
        assert_eq!((port.version.as_str(), untested), ("new", true));
    }

    #[test]
    fn a_port_with_no_mappings_still_resolves_for_the_info_modal() {
        let ports = [parse("t", YAML).unwrap()];
        let version = Version::new(1, 0, 0);
        let (port, untested) = resolve(&ports, "t", Some(&version)).unwrap();
        assert!(!port.writable());
        assert!(!untested);
    }

    #[test]
    fn invalid_range_is_rejected() {
        let text = YAML.replace("binaries = []", "binaries = []\nversions = \"latest\"");
        assert!(parse("x", &text).is_err());
    }

    #[test]
    fn paths_stay_in_config_locations() {
        for path in [
            "$XDG_CONFIG_HOME/a/config.yml",
            "~/.config/a/config.yml",
            "~/Library/Application Support/a/config.yml",
            "$LG_CONFIG_FILE",
            "$CONFIG_DIR/config.yml",
        ] {
            assert!(validate_path(path).is_ok(), "{path}");
        }
        for path in [
            "~/.ssh/config",
            "~/.bashrc",
            "/etc/passwd",
            "$HOME/.ssh/config",
            "$HOME",
            "$XDG_DATA_HOME/a",
            "$XDG_CONFIG_HOME/../.ssh/config",
            "~/.config/../.ssh/config",
            "relative/config",
        ] {
            assert!(validate_path(path).is_err(), "{path}");
        }
    }

    #[test]
    fn version_args_must_be_plain() {
        let plain = |args: &[&str]| {
            let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
            validate_version_args(&args).is_ok()
        };
        assert!(plain(&["--version"]));
        assert!(plain(&["version", "-s"]));
        assert!(!plain(&["--config=/tmp/evil"]));
        assert!(!plain(&["-c", "rm x"]));
        assert!(!plain(&["a", "b", "c", "d"]));
    }

    #[test]
    fn unknown_action_is_rejected() {
        let err = parse("x", &format!("{YAML}[target.keys]\na = \"fly\"\n")).unwrap_err();
        assert!(format!("{err:#}").contains("unknown action fly"));
    }

    #[test]
    fn unknown_role_is_rejected() {
        let err = parse("x", &format!("{YAML}[target.theme]\na = \"{{glow}}\"\n")).unwrap_err();
        assert!(format!("{err:#}").contains("unknown role glow"));
    }

    #[test]
    fn keys_need_a_notation() {
        let text = "schema = 1\nversion = \"1\"\nbinaries = []\n[[target]]\nformat = \"yaml\"\nstrategy = \"edit\"\npaths = []\n[target.keys]\na = \"up\"\n";
        let err = parse("x", text).unwrap_err();
        assert!(format!("{err:#}").contains("key_notation"));
    }

    #[test]
    fn a_template_needs_a_block_target() {
        let err = parse("x", &format!("{YAML}template = \"a\"\n")).unwrap_err();
        assert!(format!("{err:#}").contains("template"));
    }

    #[test]
    fn a_block_target_needs_a_template() {
        let text = YAML.replace("\"edit\"", "\"block\"");
        assert!(parse("x", &text).is_err());
    }

    #[test]
    fn unknown_role_in_a_template_is_rejected() {
        let text = YAML
            .replace("\"edit\"", "\"block\"")
            .replace("\"yaml\"", "\"lines\"");
        let err = parse("x", &format!("{text}template = \"{{glow}}\"\n")).unwrap_err();
        assert!(format!("{err:#}").contains("unknown role glow"));
    }

    #[test]
    fn every_file_of_a_port_counts() {
        let two = format!(
            "{YAML}[target.keys]\na = \"up\"\n[[target]]\nformat = \"toml\"\nstrategy = \"edit\"\nkey_notation = \"angle\"\npaths = []\n[target.keys]\nb = \"yank\"\n[target.theme]\nc = \"{{accent}}\"\n"
        );
        let port = parse("x", &two).unwrap();
        assert_eq!(port.targets.len(), 2);
        assert_eq!(port.action_coverage().0, ["up", "yank"]);
        assert_eq!(port.theme_settings(), 1);
        assert!(port.writable());
    }

    #[test]
    fn one_unusable_file_makes_the_port_unsupported() {
        let two = format!(
            "{YAML}[target.keys]\na = \"up\"\n[[target]]\nformat = \"json\"\nstrategy = \"edit\"\npaths = []\n[target.theme]\nc = \"x\"\n"
        );
        assert!(!parse("x", &two).unwrap().writable());
    }

    #[test]
    fn action_coverage_splits_the_vocabulary() {
        let port = parse(
            "x",
            &format!("{YAML}[target.keys]\na = \"up\"\nb = \"yank\"\n"),
        )
        .unwrap();
        let (bound, unbound) = port.action_coverage();
        assert_eq!(bound, ["up", "yank"]);
        assert_eq!(unbound, ["left", "down", "right", "delete"]);
    }

    #[test]
    fn unsupported_reason_names_what_is_missing() {
        let no_target = "schema = 1\nversion = \"1\"\nbinaries = []\n";
        let reason = |text: &str| parse("x", text).unwrap().unsupported_reason().unwrap();
        assert!(reason(no_target).contains("config"));
        assert!(reason(YAML).contains("mappings"));
    }

    #[test]
    fn writable_needs_an_editor_and_mappings() {
        let empty = parse("x", YAML).unwrap();
        let mapped = parse("x", &format!("{YAML}[target.keys]\na = \"up\"\n")).unwrap();
        let json = parse(
            "x",
            &format!("{YAML}[target.keys]\na = \"up\"\n").replace("\"yaml\"", "\"json\""),
        )
        .unwrap();

        assert!(!empty.writable());
        assert!(mapped.writable());
        assert!(!json.writable());
    }

    #[test]
    fn embedded_ports_parse() {
        let ports = embedded().unwrap();
        assert!(ports.iter().any(|p| p.name == "lazygit"));
    }

    #[test]
    fn unknown_schema_is_rejected() {
        let err = parse("x", "schema = 99\nversion = \"1\"\nbinaries = []").unwrap_err();
        assert!(err.to_string().contains("unsupported schema 99"));
    }
}
