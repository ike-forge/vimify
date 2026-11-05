use anyhow::{Result, bail};

use super::{Editor, Value};

/// `key = value` lines, for configs that are neither TOML nor YAML (`vim_keys = True`).
/// Values are verbatim, so a port writes any quotes the tool expects.
pub struct LinesEditor {
    lines: Vec<String>,
}

impl LinesEditor {
    pub fn parse(text: &str) -> Self {
        Self {
            lines: text.lines().map(str::to_owned).collect(),
        }
    }

    fn find(&self, key: &str) -> Option<usize> {
        self.lines.iter().position(|l| split(l).0 == Some(key))
    }
}

/// Key and value of a line. Comments and blank lines have neither.
fn split(line: &str) -> (Option<&str>, &str) {
    match line.split_once('=') {
        Some((key, value)) if !key.trim_start().starts_with('#') => {
            (Some(key.trim()), value.trim())
        }
        _ => (None, ""),
    }
}

impl Editor for LinesEditor {
    fn set(&mut self, path: &[&str], value: Value) -> Result<()> {
        let key = path.join(".");
        let Value::Str(value) = value else {
            bail!("{key}: a list has no form in a lines config");
        };
        match self.find(&key) {
            Some(i) => {
                let prefix = self.lines[i].split_once('=').map_or("", |(p, _)| p);
                self.lines[i] = format!("{prefix}= {value}");
            }
            None => self.lines.push(format!("{key} = {value}")),
        }
        Ok(())
    }

    fn get(&self, path: &[&str]) -> Option<Value> {
        let line = &self.lines[self.find(&path.join("."))?];
        Some(Value::Str(split(line).1.to_owned()))
    }

    fn restore(&mut self, path: &[&str], original: &str) -> Result<()> {
        let key = path.join(".");
        let original = Self::parse(original);
        let line = original.find(&key).map(|i| original.lines[i].clone());
        match (self.find(&key), line) {
            (Some(i), Some(line)) => self.lines[i] = line,
            (Some(i), None) => {
                self.lines.remove(i);
            }
            (None, Some(line)) => self.lines.push(line),
            (None, None) => {}
        }
        Ok(())
    }

    fn render(&self) -> String {
        self.lines.iter().map(|l| format!("{l}\n")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn str(s: &str) -> Value {
        Value::Str(s.into())
    }

    fn edit(text: &str, key: &str, value: &str) -> String {
        let mut editor = LinesEditor::parse(text);
        editor.set(&[key], str(value)).unwrap();
        editor.render()
    }

    fn restore(text: &str, key: &str, original: &str) -> String {
        let mut editor = LinesEditor::parse(text);
        editor.restore(&[key], original).unwrap();
        editor.render()
    }

    #[test]
    fn replaces_a_value_and_keeps_everything_else() {
        let text = "#? Config\n\n#* Help\nvim_keys = False\nrounded = True\n";
        assert_eq!(
            edit(text, "vim_keys", "True"),
            "#? Config\n\n#* Help\nvim_keys = True\nrounded = True\n"
        );
    }

    #[test]
    fn keeps_the_spacing_before_the_equals() {
        assert_eq!(edit("a   = 1\n", "a", "2"), "a   = 2\n");
    }

    #[test]
    fn appends_a_missing_key() {
        assert_eq!(edit("a = 1\n", "b", "2"), "a = 1\nb = 2\n");
        assert_eq!(edit("", "b", "2"), "b = 2\n");
    }

    #[test]
    fn a_commented_key_is_not_the_key() {
        assert_eq!(edit("#a = 1\n", "a", "2"), "#a = 1\na = 2\n");
    }

    #[test]
    fn keys_keep_their_brackets() {
        let out = edit(
            "theme[main_bg]=\"#000000\"\n",
            "theme[main_bg]",
            "\"#303446\"",
        );
        assert_eq!(out, "theme[main_bg]= \"#303446\"\n");
    }

    #[test]
    fn a_list_is_refused() {
        let mut editor = LinesEditor::parse("");
        assert!(editor.set(&["a"], Value::List(vec![])).is_err());
    }

    #[test]
    fn get_ignores_spacing() {
        let editor = LinesEditor::parse("a=1\nb =  \"x\"\n");
        assert_eq!(editor.get(&["a"]), Some(str("1")));
        assert_eq!(editor.get(&["b"]), Some(str("\"x\"")));
        assert_eq!(editor.get(&["c"]), None);
    }

    #[test]
    fn restore_brings_the_line_back_exactly() {
        assert_eq!(restore("a = 2\n", "a", "a=1\n"), "a=1\n");
    }

    #[test]
    fn restore_removes_a_key_the_original_lacks() {
        assert_eq!(restore("a = 1\nb = 2\n", "a", "b = 2\n"), "b = 2\n");
    }

    #[test]
    fn restore_returns_a_removed_key() {
        assert_eq!(restore("b = 2\n", "a", "a = 1\n"), "b = 2\na = 1\n");
    }

    #[test]
    fn setting_twice_is_idempotent() {
        let once = edit("", "a", "1");
        assert_eq!(edit(&once, "a", "1"), once);
    }
}
