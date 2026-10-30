mod toml;
mod yaml;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::port::Format;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Str(String),
    List(Vec<String>),
}

/// Keeps format crates swappable; see stack.md on `yaml-edit`.
pub trait Editor {
    fn set(&mut self, path: &[&str], value: Value) -> Result<()>;
    /// `None` if the path is absent or runs through something other than a mapping.
    fn get(&self, path: &[&str]) -> Option<Value>;
    /// Makes `path` match `original`, a file of the same format: copies its value, or
    /// removes the key and any parent left empty that `original` does not have.
    fn restore(&mut self, path: &[&str], original: &str) -> Result<()>;
    fn render(&self) -> String;
}

pub fn open(format: Format, text: &str) -> Result<Box<dyn Editor>> {
    match format {
        Format::Yaml => Ok(Box::new(yaml::YamlEditor::parse(text)?)),
        Format::Toml => Ok(Box::new(toml::TomlEditor::parse(text)?)),
        Format::Json | Format::Lines => bail!("no editor for {format:?}"),
    }
}
