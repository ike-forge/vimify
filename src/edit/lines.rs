use anyhow::{Result, bail};

use super::{Editor, Value};

/// Line-per-setting configs that are neither TOML nor YAML. Values are verbatim, so a
/// port writes any quotes the tool expects.
pub struct LinesEditor {
    lines: Vec<String>,
    syntax: Syntax,
}

#[derive(Clone, Copy)]
enum Syntax {
    /// `vim_keys = True`
    Assign,
    /// Command-line flags, `--theme="ansi"` or `--theme ansi`, as in bat's config.
    Args,
}

impl LinesEditor {
    pub fn parse(text: &str) -> Self {
        Self::with(text, Syntax::Assign)
    }

    pub fn args(text: &str) -> Self {
        Self::with(text, Syntax::Args)
    }

    fn with(text: &str, syntax: Syntax) -> Self {
        Self {
            lines: text.lines().map(str::to_owned).collect(),
            syntax,
        }
    }

    fn reparse(&self, text: &str) -> Self {
        Self::with(text, self.syntax)
    }

    fn find(&self, key: &str) -> Option<usize> {
        let mut matches = self.lines.iter().map(|l| self.split(l).0 == Some(key));
        match self.syntax {
            Syntax::Assign => matches.position(|m| m),
            // The last flag wins, so it is the one to replace.
            Syntax::Args => matches.rposition(|m| m),
        }
    }

    /// Key and value of a line. Comments and blank lines have neither.
    fn split<'a>(&self, line: &'a str) -> (Option<&'a str>, &'a str) {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            return (None, "");
        }
        let parts = match self.syntax {
            Syntax::Assign => line.split_once('='),
            Syntax::Args => trimmed
                .find(|c: char| c == '=' || c.is_whitespace())
                .map(|i| (&trimmed[..i], &trimmed[i + 1..]))
                .or(Some((trimmed, "")))
                .filter(|(key, _)| key.starts_with('-')),
        };
        match parts {
            Some((key, value)) => (Some(key.trim()), value.trim()),
            None => (None, ""),
        }
    }

    fn line(&self, key: &str, value: &str) -> String {
        match self.syntax {
            Syntax::Assign => format!("{key} = {value}"),
            Syntax::Args => format!("{key}={value}"),
        }
    }
}

impl Editor for LinesEditor {
    fn set(&mut self, path: &[&str], value: Value) -> Result<()> {
        let key = path.join(".");
        let Value::Str(value) = value else {
            bail!("{key}: only text has a form in a lines config");
        };
        let line = match (self.syntax, self.find(&key)) {
            (Syntax::Assign, Some(i)) => {
                let prefix = self.lines[i].split_once('=').map_or("", |(p, _)| p);
                format!("{prefix}= {value}")
            }
            _ => self.line(&key, &value),
        };
        match self.find(&key) {
            Some(i) => self.lines[i] = line,
            None => self.lines.push(line),
        }
        Ok(())
    }

    fn get(&self, path: &[&str]) -> Option<Value> {
        let line = &self.lines[self.find(&path.join("."))?];
        Some(Value::Str(self.split(line).1.to_owned()))
    }

    fn restore(&mut self, path: &[&str], original: &str) -> Result<()> {
        let key = path.join(".");
        let original = self.reparse(original);
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

    fn edit_args(text: &str, key: &str, value: &str) -> String {
        let mut editor = LinesEditor::args(text);
        editor.set(&[key], str(value)).unwrap();
        editor.render()
    }

    #[test]
    fn args_replace_the_flag_in_place() {
        let text = "# Mine\n--theme=\"Catppuccin Frappe\"\n--paging=never\n";
        assert_eq!(
            edit_args(text, "--theme", "\"ansi\""),
            "# Mine\n--theme=\"ansi\"\n--paging=never\n"
        );
    }

    #[test]
    fn args_read_a_space_separated_flag() {
        assert_eq!(
            edit_args("--theme TwoDark\n", "--theme", "x"),
            "--theme=x\n"
        );
        let editor = LinesEditor::args("  --theme   TwoDark\n");
        assert_eq!(editor.get(&["--theme"]), Some(str("TwoDark")));
    }

    #[test]
    fn args_replace_the_last_of_repeated_flags() {
        assert_eq!(
            edit_args("--theme=a\n--theme=b\n", "--theme", "x"),
            "--theme=a\n--theme=x\n"
        );
    }

    #[test]
    fn args_append_a_missing_flag() {
        assert_eq!(
            edit_args("--paging=never\n", "--theme", "x"),
            "--paging=never\n--theme=x\n"
        );
    }

    #[test]
    fn args_ignore_comments_and_other_flags() {
        assert_eq!(
            edit_args("#--theme=a\n--theme-dark=b\n", "--theme", "x"),
            "#--theme=a\n--theme-dark=b\n--theme=x\n"
        );
    }

    #[test]
    fn args_restore_the_original_line() {
        let mut editor = LinesEditor::args("--theme=x\n");
        editor
            .restore(&["--theme"], "--theme \"Two Dark\"\n")
            .unwrap();
        assert_eq!(editor.render(), "--theme \"Two Dark\"\n");
    }

    #[test]
    fn setting_twice_is_idempotent() {
        let once = edit("", "a", "1");
        assert_eq!(edit(&once, "a", "1"), once);
    }
}
