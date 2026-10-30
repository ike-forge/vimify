mod yaml;

use anyhow::{Result, bail};

use crate::port::Format;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Str(String),
    List(Vec<String>),
}

/// Keeps format crates swappable; see stack.md on `yaml-edit`.
pub trait Editor {
    fn set(&mut self, path: &[&str], value: Value) -> Result<()>;
    fn render(&self) -> String;
}

pub fn open(format: Format, text: &str) -> Result<Box<dyn Editor>> {
    match format {
        Format::Yaml => Ok(Box::new(yaml::YamlEditor::parse(text)?)),
        Format::Toml | Format::Json | Format::Lines => bail!("no editor for {format:?}"),
    }
}
