mod block;
mod file;
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

pub fn merges(format: Format) -> bool {
    format == Format::Yaml
}

/// The settings of an upstream theme file, for a target that merges it key by key.
pub fn settings(format: Format, text: &str) -> Result<Vec<(String, Value)>> {
    match format {
        Format::Yaml => yaml::settings(text),
        _ => bail!("no settings reader for {format:?}"),
    }
}

/// Splits a dotted setting; dots inside `[...]` belong to the segment.
pub fn split(setting: &str) -> Vec<&str> {
    let mut segments = Vec::new();
    let (mut start, mut depth) = (0, 0usize);
    for (i, c) in setting.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            '.' if depth == 0 => {
                segments.push(&setting[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    segments.push(&setting[start..]);
    segments
}

pub fn supports(format: Format, strategy: Strategy) -> bool {
    matches!(
        (format, strategy),
        (
            Format::Yaml | Format::Toml | Format::Ron | Format::Lines | Format::Args,
            Strategy::Edit
        ) | (Format::Lines, Strategy::Block)
            | (_, Strategy::File)
    )
}

pub fn open(format: Format, strategy: Strategy, text: &str) -> Result<Box<dyn Editor>> {
    match (format, strategy) {
        (Format::Yaml, Strategy::Edit) => Ok(Box::new(yaml::YamlEditor::parse(text)?)),
        (Format::Toml, Strategy::Edit) => Ok(Box::new(toml::TomlEditor::parse(text)?)),
        (Format::Ron, Strategy::Edit) => Ok(Box::new(ron::RonEditor::parse(text)?)),
        (Format::Lines, Strategy::Edit) => Ok(Box::new(lines::LinesEditor::parse(text))),
        (Format::Args, Strategy::Edit) => Ok(Box::new(lines::LinesEditor::args(text))),
        (Format::Lines, Strategy::Block) => Ok(Box::new(block::BlockEditor::parse(text)?)),
        (_, Strategy::File) => Ok(Box::new(file::FileEditor::parse(text))),
        _ => bail!("no editor for {format:?} with {strategy:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_keeps_dots_inside_brackets() {
        assert_eq!(split("a.b"), ["a", "b"]);
        assert_eq!(
            split("general.import[=~/.config/x.toml]"),
            ["general", "import[=~/.config/x.toml]"]
        );
        assert_eq!(
            split("m.k[run=arrow 50%].on"),
            ["m", "k[run=arrow 50%]", "on"]
        );
    }
}
