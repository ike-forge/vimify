use std::str::FromStr;

use anyhow::{Context, Result, anyhow, bail};
use yaml_edit::{Document, Mapping, YamlValue};

use super::{Editor, Value};

pub struct YamlEditor {
    /// Comments of a file with no content, which `Document` cannot hold.
    prefix: String,
    doc: Document,
}

impl YamlEditor {
    pub fn parse(text: &str) -> Result<Self> {
        if text.lines().all(is_blank_or_comment) {
            let mut prefix = text.to_owned();
            if !prefix.is_empty() && !prefix.ends_with('\n') {
                prefix.push('\n');
            }
            return Ok(Self {
                prefix,
                doc: Document::new_mapping(),
            });
        }
        let doc = Document::from_str(text).map_err(|e| anyhow!("{e}"))?;
        if doc.as_mapping().is_none() {
            bail!("top level is not a mapping");
        }
        Ok(Self {
            prefix: String::new(),
            doc,
        })
    }
}

impl Editor for YamlEditor {
    fn set(&mut self, path: &[&str], value: Value) -> Result<()> {
        let root = self.doc.as_mapping().context("no top-level mapping")?;
        set_in(&root, path, &value).with_context(|| path.join("."))
    }

    fn render(&self) -> String {
        format!("{}{}", self.prefix, self.doc)
    }
}

fn is_blank_or_comment(line: &str) -> bool {
    let line = line.trim();
    line.is_empty() || line.starts_with('#')
}

fn set_in(mapping: &Mapping, path: &[&str], value: &Value) -> Result<()> {
    let [key, rest @ ..] = path else {
        bail!("empty path");
    };
    if rest.is_empty() {
        mapping.set(*key, to_yaml(value));
        return Ok(());
    }
    match mapping.get(*key) {
        None => {
            mapping.set(*key, nested(rest, value));
            Ok(())
        }
        Some(node) if node.as_scalar().is_some_and(|s| s.is_null()) => {
            mapping.set(*key, nested(rest, value));
            Ok(())
        }
        Some(node) => match node.as_mapping() {
            Some(child) => set_in(child, rest, value),
            // Not vimify's value to replace: `Mapping::set` would silently drop it.
            None => bail!("{key} is not a mapping"),
        },
    }
}

fn nested(path: &[&str], value: &Value) -> YamlValue {
    match path {
        [] => to_yaml(value),
        [key, rest @ ..] => YamlValue::Mapping([((*key).to_owned(), nested(rest, value))].into()),
    }
}

fn to_yaml(value: &Value) -> YamlValue {
    match value {
        Value::Str(s) => YamlValue::from(s.as_str()),
        Value::List(items) => {
            YamlValue::Sequence(items.iter().map(|s| YamlValue::from(s.as_str())).collect())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(text: &str, path: &[&str], value: Value) -> Result<String> {
        let mut editor = YamlEditor::parse(text)?;
        editor.set(path, value)?;
        Ok(editor.render())
    }

    fn str(s: &str) -> Value {
        Value::Str(s.into())
    }

    fn list(items: &[&str]) -> Value {
        Value::List(items.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn replaces_value_and_keeps_comments_and_order() {
        let text = "# top\na:\n  # mine\n  b: old\n  c: 1 # keep\n\nd: \"quoted\"\n";
        let out = edit(text, &["a", "b"], str("new")).unwrap();
        assert_eq!(
            out,
            "# top\na:\n  # mine\n  b: new\n  c: 1 # keep\n\nd: \"quoted\"\n"
        );
    }

    #[test]
    fn creates_missing_parents() {
        let out = edit("a: 1\n", &["b", "c", "d"], str("x")).unwrap();
        assert_eq!(out, "a: 1\nb:\n  c:\n    d: x\n");
    }

    #[test]
    fn inserts_into_existing_parent() {
        let out = edit("a:\n  b: 1\n", &["a", "c"], str("x")).unwrap();
        assert_eq!(out, "a:\n  b: 1\n  c: x\n");
    }

    #[test]
    fn null_parent_becomes_a_mapping() {
        let out = edit("a:\nb: 1\n", &["a", "c"], str("x")).unwrap();
        assert_eq!(out, "a:\n  c: x\nb: 1\n");
    }

    #[test]
    fn scalar_parent_is_refused() {
        let err = edit("a: 1\n", &["a", "b"], str("x")).unwrap_err();
        assert!(format!("{err:#}").contains("a is not a mapping"));
    }

    #[test]
    fn sequence_parent_is_refused() {
        assert!(edit("a:\n  - 1\n", &["a", "b"], str("x")).is_err());
    }

    #[test]
    fn replaces_sequence() {
        let out = edit("a:\n  - old\n", &["a"], list(&["#8caaee", "bold"])).unwrap();
        assert_eq!(out, "a:\n  - '#8caaee'\n  - bold\n");
    }

    #[test]
    fn keeps_flow_style() {
        let out = edit("a: {b: old, c: 1}\n", &["a", "b"], str("new")).unwrap();
        assert_eq!(out, "a: {b: new, c: 1}\n");
    }

    #[test]
    fn empty_file_gets_content() {
        assert_eq!(edit("", &["a", "b"], str("x")).unwrap(), "a:\n  b: x\n");
    }

    #[test]
    fn comment_only_file_keeps_comments() {
        let out = edit("# mine\n\n# more", &["a"], str("x")).unwrap();
        assert_eq!(out, "# mine\n\n# more\na: x\n");
    }

    #[test]
    fn key_notation_survives_a_reparse() {
        let out = edit("a: 1\n", &["k"], str("<c-u>")).unwrap();
        let doc = Document::from_str(&out).unwrap();
        assert_eq!(doc.get_string("k").as_deref(), Some("<c-u>"));
    }

    #[test]
    fn top_level_sequence_is_refused() {
        assert!(YamlEditor::parse("- a\n").is_err());
    }

    #[test]
    fn setting_twice_is_idempotent() {
        let once = edit("a: 1\n", &["b", "c"], list(&["x"])).unwrap();
        let twice = edit(&once, &["b", "c"], list(&["x"])).unwrap();
        assert_eq!(once, twice);
    }
}
