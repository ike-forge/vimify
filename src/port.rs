use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use include_dir::{Dir, include_dir};
use serde::Deserialize;

use crate::paths;

static EMBEDDED: Dir = include_dir!("$CARGO_MANIFEST_DIR/ports");

pub const SCHEMA: u32 = 1;

#[derive(Debug, Clone)]
pub struct Port {
    pub name: String,
    pub version: String,
    pub binaries: Vec<String>,
    pub target: Option<Target>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Target {
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
    Ok(Port {
        name: name.to_owned(),
        version: file.version,
        binaries: file.binaries,
        target: file.target,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
