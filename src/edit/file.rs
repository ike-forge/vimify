use anyhow::{Result, bail};

use super::{Editor, Value};

/// A file vimify writes whole: one it owns, or a tool's theme file it takes over.
pub struct FileEditor {
    text: String,
}

impl FileEditor {
    pub fn parse(text: &str) -> Self {
        Self {
            text: text.to_owned(),
        }
    }
}

impl Editor for FileEditor {
    fn set(&mut self, _path: &[&str], value: Value) -> Result<()> {
        let Value::Str(text) = value else {
            bail!("a file is text");
        };
        self.text = format!("{}\n", text.trim_end_matches('\n'));
        Ok(())
    }

    fn get(&self, _path: &[&str]) -> Option<Value> {
        (!self.text.is_empty()).then(|| Value::Str(self.text.trim_end_matches('\n').to_owned()))
    }

    fn restore(&mut self, _path: &[&str], original: &str) -> Result<()> {
        self.text = original.to_owned();
        Ok(())
    }

    fn render(&self) -> String {
        self.text.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_replaces_everything() {
        let mut editor = FileEditor::parse("<old/>\n");
        editor.set(&[], Value::Str("<new/>".into())).unwrap();
        assert_eq!(editor.render(), "<new/>\n");
        assert_eq!(editor.get(&[]), Some(Value::Str("<new/>".into())));
    }

    #[test]
    fn an_empty_file_has_no_value() {
        assert_eq!(FileEditor::parse("").get(&[]), None);
    }

    #[test]
    fn restore_brings_back_the_original() {
        let mut editor = FileEditor::parse("<new/>\n");
        editor.restore(&[], "<old/>").unwrap();
        assert_eq!(editor.render(), "<old/>");
    }
}
