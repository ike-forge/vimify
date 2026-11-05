mod block;
mod lines;
mod ron;
mod toml;
mod yaml;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::port::{Format, Strategy};

/// The one setting of a `block` target: its whole text.
pub const BLOCK: &str = "block";

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

pub fn supports(format: Format, strategy: Strategy) -> bool {
    matches!(
        (format, strategy),
        (
            Format::Yaml | Format::Toml | Format::Ron | Format::Lines,
            Strategy::Edit
        ) | (Format::Lines, Strategy::Block)
    )
}

pub fn open(format: Format, strategy: Strategy, text: &str) -> Result<Box<dyn Editor>> {
    match (format, strategy) {
        (Format::Yaml, Strategy::Edit) => Ok(Box::new(yaml::YamlEditor::parse(text)?)),
        (Format::Toml, Strategy::Edit) => Ok(Box::new(toml::TomlEditor::parse(text)?)),
        (Format::Ron, Strategy::Edit) => Ok(Box::new(ron::RonEditor::parse(text)?)),
        (Format::Lines, Strategy::Edit) => Ok(Box::new(lines::LinesEditor::parse(text))),
        (Format::Lines, Strategy::Block) => Ok(Box::new(block::BlockEditor::parse(text)?)),
        _ => bail!("no editor for {format:?} with {strategy:?}"),
    }
}
