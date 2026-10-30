use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use include_dir::{Dir, include_dir};
use serde::Deserialize;

use crate::actions::{self, Notation};
use crate::{palette, paths};

static EMBEDDED: Dir = include_dir!("$CARGO_MANIFEST_DIR/ports");

pub const SCHEMA: u32 = 1;

#[derive(Debug, Clone)]
pub struct Port {
    pub name: String,
    pub version: String,
    pub binaries: Vec<String>,
    pub target: Option<Target>,
    /// Dotted tool setting → action.
    pub keys: BTreeMap<String, String>,
    /// Dotted tool setting → `{role}` or literal values.
    pub theme: BTreeMap<String, ThemeValue>,
}

impl Port {
    /// False until an editor exists for the port and it has mappings to write.
    pub fn writable(&self) -> bool {
        let has_mappings = !self.keys.is_empty() || !self.theme.is_empty();
        has_mappings
            && self
                .target
                .as_ref()
                .is_some_and(|t| t.format == Format::Yaml && t.strategy == Strategy::Edit)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum ThemeValue {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Toml,
    Yaml,
    Json,
    Lines,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
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
}

impl Target {
    /// First existing path, else the first that expands (to be created).
    /// The bool is whether the file exists.
    pub fn resolve(&self) -> Option<(PathBuf, bool)> {
        let candidates: Vec<PathBuf> = self.paths.iter().filter_map(|p| paths::expand(p)).collect();
        if let Some(existing) = candidates.iter().find(|p| p.exists()) {
            return Some((existing.clone(), true));
        }
        candidates.into_iter().next().map(|p| (p, false))
    }
}

#[derive(Deserialize)]
struct PortFile {
    schema: u32,
    version: String,
    binaries: Vec<String>,
    target: Option<Target>,
    #[serde(default)]
    keys: BTreeMap<String, String>,
    #[serde(default)]
    theme: BTreeMap<String, ThemeValue>,
}

/// Ports compiled into the binary, sorted by name.
pub fn embedded() -> Result<Vec<Port>> {
    let mut ports = EMBEDDED
        .dirs()
        .map(|dir| {
            let name = dir.path().to_string_lossy();
            let text = dir
                .get_file(dir.path().join("port.toml"))
                .and_then(|f| f.contents_utf8())
                .with_context(|| format!("port {name}: missing port.toml"))?;
            parse(&name, text)
        })
        .collect::<Result<Vec<_>>>()?;
    ports.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(ports)
}

pub fn parse(name: &str, text: &str) -> Result<Port> {
    let file: PortFile = toml::from_str(text).with_context(|| format!("port {name}"))?;
    if file.schema != SCHEMA {
        bail!("port {name}: unsupported schema {}", file.schema);
    }
    let port = Port {
        name: name.to_owned(),
        version: file.version,
        binaries: file.binaries,
        target: file.target,
        keys: file.keys,
        theme: file.theme,
    };
    validate(&port).with_context(|| format!("port {name}"))?;
    Ok(port)
}

/// Ports cannot define new actions or roles.
fn validate(port: &Port) -> Result<()> {
    for action in port.keys.values() {
        if !actions::is_action(action) {
            bail!("unknown action {action}");
        }
    }
    let notation = port.target.as_ref().and_then(|t| t.key_notation);
    if !port.keys.is_empty() && notation.is_none() {
        bail!("keys need a key_notation");
    }
    for value in port.theme.values() {
        let items = match value {
            ThemeValue::One(s) => std::slice::from_ref(s),
            ThemeValue::Many(items) => items.as_slice(),
        };
        for role in items.iter().filter_map(|s| palette::role_of(s)) {
            if !palette::ROLES.contains(&role) {
                bail!("unknown role {role}");
            }
        }
    }
    Ok(())
}

/// Port name, input and expected text of every `tests/*.input.*` file.
#[cfg(test)]
pub fn golden_cases() -> Vec<(String, &'static str, &'static str)> {
    let mut cases = Vec::new();
    for dir in EMBEDDED.dirs() {
        let name = dir.path().to_string_lossy().into_owned();
        let Some(tests) = EMBEDDED.get_dir(format!("{name}/tests")) else {
            continue;
        };
        for file in tests.files() {
            let path = file.path().to_string_lossy();
            if let Some((stem, ext)) = path.rsplit_once(".input.") {
                let expected = EMBEDDED
                    .get_file(format!("{stem}.expected.{ext}"))
                    .and_then(|f| f.contents_utf8())
                    .unwrap_or_else(|| panic!("{path} has no expected file"));
                cases.push((name.clone(), file.contents_utf8().unwrap(), expected));
            }
        }
    }
    cases
}

#[cfg(test)]
mod tests {
    use super::*;

    const YAML: &str = "schema = 1\nversion = \"1\"\nbinaries = []\n[target]\nformat = \"yaml\"\nstrategy = \"edit\"\nkey_notation = \"angle\"\npaths = []\n";

    #[test]
    fn unknown_action_is_rejected() {
        let err = parse("x", &format!("{YAML}[keys]\na = \"fly\"\n")).unwrap_err();
        assert!(format!("{err:#}").contains("unknown action fly"));
    }

    #[test]
    fn unknown_role_is_rejected() {
        let err = parse("x", &format!("{YAML}[theme]\na = \"{{glow}}\"\n")).unwrap_err();
        assert!(format!("{err:#}").contains("unknown role glow"));
    }

    #[test]
    fn keys_need_a_notation() {
        let text = "schema = 1\nversion = \"1\"\nbinaries = []\n[target]\nformat = \"yaml\"\nstrategy = \"edit\"\npaths = []\n[keys]\na = \"up\"\n";
        let err = parse("x", text).unwrap_err();
        assert!(format!("{err:#}").contains("key_notation"));
    }

    #[test]
    fn writable_needs_an_editor_and_mappings() {
        let empty = parse("x", YAML).unwrap();
        let mapped = parse("x", &format!("{YAML}[keys]\na = \"up\"\n")).unwrap();
        let toml = parse(
            "x",
            &format!("{YAML}[keys]\na = \"up\"\n").replace("\"yaml\"", "\"toml\""),
        )
        .unwrap();

        assert!(!empty.writable());
        assert!(mapped.writable());
        assert!(!toml.writable());
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
