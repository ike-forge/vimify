use std::ops::Range;

use anyhow::{Result, bail};

use super::{Editor, Value};

/// A RON file whose root is a struct of fields (`( name: value, ... )`), edited in place.
/// Values are verbatim RON; settings are top-level field names.
pub struct RonEditor {
    text: String,
}

struct Field {
    name: String,
    /// From the name to the end of the entry, trailing comma included.
    entry: Range<usize>,
    value: Range<usize>,
    comma: bool,
}

struct Root {
    fields: Vec<Field>,
    open: usize,
    close: usize,
}

impl RonEditor {
    pub fn parse(text: &str) -> Result<Self> {
        if !text.trim().is_empty() {
            root(text)?;
        }
        Ok(Self {
            text: text.to_owned(),
        })
    }

    fn root(&self) -> Root {
        root(&self.text).unwrap_or(Root {
            fields: Vec::new(),
            open: 0,
            close: 0,
        })
    }

    fn ensure_root(&mut self) {
        if self.text.trim().is_empty() {
            self.text = "()\n".to_owned();
        }
    }

    fn insert(&mut self, name: &str, value: &str) {
        let root = self.root();
        let Some(last) = root.fields.last() else {
            let inner = root.open + 1..root.close;
            let new = format!("\n    {name}: {value},\n");
            if self.text[inner.clone()].trim().is_empty() {
                self.text.replace_range(inner, &new);
            } else {
                self.text.insert_str(root.close, &new);
            }
            return;
        };
        let tail = &self.text[last.entry.end..root.close];
        let Some(newline) = tail.find('\n') else {
            // One-line struct.
            if last.comma {
                self.text
                    .insert_str(last.entry.end, &format!(" {name}: {value},"));
            } else {
                self.text
                    .insert_str(last.value.end, &format!(", {name}: {value}"));
            }
            return;
        };
        let line = self.text[..last.entry.start]
            .rfind('\n')
            .map_or(0, |i| i + 1);
        let indent = &self.text[line..last.entry.start];
        let indent = if indent.trim().is_empty() {
            indent.to_owned()
        } else {
            "    ".into()
        };
        let at = last.entry.end + newline + 1;
        if !last.comma {
            self.text.insert(last.value.end, ',');
            self.text
                .insert_str(at + 1, &format!("{indent}{name}: {value},\n"));
        } else {
            self.text
                .insert_str(at, &format!("{indent}{name}: {value},\n"));
        }
    }

    fn remove(&mut self, name: &str) {
        let root = self.root();
        let Some(field) = root.fields.iter().find(|f| f.name == name) else {
            return;
        };
        let mut range = field.entry.clone();
        let line = self.text[..range.start].rfind('\n').map_or(0, |i| i + 1);
        let rest = &self.text[range.end..];
        let eol = rest.find('\n').unwrap_or(rest.len());
        if self.text[line..range.start].trim().is_empty() && rest[..eol].trim().is_empty() {
            range = line..(range.end + (eol + 1).min(rest.len()));
        } else if let Some(prev) = root
            .fields
            .iter()
            .rev()
            .skip_while(|f| f.name != name)
            .nth(1)
        {
            let from = if field.comma {
                prev.entry.end
            } else {
                prev.value.end
            };
            range = from..range.end;
        }
        self.text.replace_range(range, "");
        let root = self.root();
        if root.fields.is_empty() && self.text[root.open + 1..root.close].trim().is_empty() {
            self.text.replace_range(root.open + 1..root.close, "");
        }
    }
}

fn name(path: &[&str]) -> Result<String> {
    match path {
        [name] => Ok((*name).to_owned()),
        _ => bail!("{}: a RON setting is one top-level field", path.join(".")),
    }
}

impl Editor for RonEditor {
    fn set(&mut self, path: &[&str], value: Value) -> Result<()> {
        let name = name(path)?;
        let Value::Str(value) = value else {
            bail!("{name}: a list has no form in a RON config");
        };
        self.ensure_root();
        match self.root().fields.iter().find(|f| f.name == name) {
            Some(field) => self.text.replace_range(field.value.clone(), &value),
            None => self.insert(&name, &value),
        }
        Ok(())
    }

    fn get(&self, path: &[&str]) -> Option<Value> {
        let name = name(path).ok()?;
        let root = self.root();
        let field = root.fields.iter().find(|f| f.name == name)?;
        Some(Value::Str(self.text[field.value.clone()].to_owned()))
    }

    fn restore(&mut self, path: &[&str], original: &str) -> Result<()> {
        let original = Self::parse(original)?;
        match original.get(path) {
            Some(value) => self.set(path, value),
            None => {
                self.remove(&name(path)?);
                Ok(())
            }
        }
    }

    fn render(&self) -> String {
        self.text.clone()
    }
}

fn root(text: &str) -> Result<Root> {
    let b = text.as_bytes();
    let mut i = trivia(text, 0)?;
    while text[i..].starts_with("#!") {
        // `#![enable(...)]`
        i += text[i..].find(']').map_or(text.len() - i, |n| n + 1);
        i = trivia(text, i)?;
    }
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
        i += 1;
    }
    i = trivia(text, i)?;
    if b.get(i) != Some(&b'(') {
        bail!("RON root is not a struct");
    }
    let open = i;
    i += 1;
    let mut fields = Vec::new();
    loop {
        i = trivia(text, i)?;
        match b.get(i) {
            Some(b')') => {
                return Ok(Root {
                    fields,
                    open,
                    close: i,
                });
            }
            None => bail!("unclosed RON struct"),
            _ => {}
        }
        let start = i;
        while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
            i += 1;
        }
        let name = text[start..i].to_owned();
        i = trivia(text, i)?;
        if name.is_empty() || b.get(i) != Some(&b':') {
            bail!("RON field expected at byte {start}");
        }
        i = trivia(text, i + 1)?;
        let value_start = i;
        i = value_end(text, i)?;
        let mut value_end = i;
        while value_end > value_start && b[value_end - 1].is_ascii_whitespace() {
            value_end -= 1;
        }
        let comma = b.get(i) == Some(&b',');
        if comma {
            i += 1;
        } else {
            i = value_end;
        }
        fields.push(Field {
            name,
            entry: start..i,
            value: value_start..value_end,
            comma,
        });
    }
}

/// Skips whitespace and `//` and nesting `/* */` comments.
fn trivia(text: &str, mut i: usize) -> Result<usize> {
    let b = text.as_bytes();
    loop {
        match b.get(i) {
            Some(c) if c.is_ascii_whitespace() => i += 1,
            Some(b'/') if b.get(i + 1) == Some(&b'/') => {
                i += text[i..].find('\n').unwrap_or(text.len() - i);
            }
            Some(b'/') if b.get(i + 1) == Some(&b'*') => {
                let mut depth = 0;
                loop {
                    if text[i..].starts_with("/*") {
                        depth += 1;
                        i += 2;
                    } else if text[i..].starts_with("*/") {
                        depth -= 1;
                        i += 2;
                    } else if i >= b.len() {
                        bail!("unclosed RON comment");
                    } else {
                        i += 1;
                    }
                    if depth == 0 {
                        break;
                    }
                }
            }
            _ => return Ok(i),
        }
    }
}

/// End of a field value: the `,` or `)` at depth zero.
fn value_end(text: &str, mut i: usize) -> Result<usize> {
    let b = text.as_bytes();
    let mut depth = 0usize;
    while let Some(&c) = b.get(i) {
        match c {
            b'"' | b'\'' => i = quoted(text, i)?,
            b'r' if matches!(b.get(i + 1), Some(b'"' | b'#')) && raw_start(text, i) => {
                i = raw(text, i)?;
            }
            b'/' if matches!(b.get(i + 1), Some(b'/' | b'*')) => i = trivia(text, i)?,
            b'(' | b'[' | b'{' => {
                depth += 1;
                i += 1;
            }
            b')' | b']' | b'}' if depth > 0 => {
                depth -= 1;
                i += 1;
            }
            b',' | b')' if depth == 0 => return Ok(i),
            _ => i += 1,
        }
    }
    bail!("unclosed RON value")
}

fn raw_start(text: &str, i: usize) -> bool {
    let rest = text[i + 1..].trim_start_matches('#');
    rest.starts_with('"') && !text[..i].ends_with(|c: char| c.is_ascii_alphanumeric() || c == '_')
}

fn raw(text: &str, i: usize) -> Result<usize> {
    let hashes = text[i + 1..].len() - text[i + 1..].trim_start_matches('#').len();
    let close = format!("\"{}", "#".repeat(hashes));
    let body = i + 1 + hashes + 1;
    match text[body..].find(&close) {
        Some(n) => Ok(body + n + close.len()),
        None => bail!("unclosed RON raw string"),
    }
}

/// End of the string or char literal opening at `i`.
fn quoted(text: &str, i: usize) -> Result<usize> {
    let b = text.as_bytes();
    let quote = b[i];
    let mut j = i + 1;
    while let Some(&c) = b.get(j) {
        match c {
            b'\\' => j += 2,
            c if c == quote => return Ok(j + 1),
            _ => j += 1,
        }
    }
    bail!("unclosed RON literal")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn str(s: &str) -> Value {
        Value::Str(s.into())
    }

    fn edit(text: &str, name: &str, value: &str) -> String {
        let mut e = RonEditor::parse(text).unwrap();
        e.set(&[name], str(value)).unwrap();
        e.render()
    }

    #[test]
    fn replaces_a_value_and_keeps_comments() {
        let text = "(\n    // mine\n    a: Some(\"x, )\"), // trailing\n    b: 2,\n)\n";
        assert_eq!(
            edit(text, "a", "Some(\"y\")"),
            "(\n    // mine\n    a: Some(\"y\"), // trailing\n    b: 2,\n)\n"
        );
    }

    #[test]
    fn appends_after_the_last_field() {
        assert_eq!(
            edit("(\n    a: 1,\n)\n", "b", "2"),
            "(\n    a: 1,\n    b: 2,\n)\n"
        );
        assert_eq!(
            edit("(\n    a: 1\n)\n", "b", "2"),
            "(\n    a: 1,\n    b: 2,\n)\n"
        );
    }

    #[test]
    fn appends_to_a_one_line_struct() {
        assert_eq!(edit("(a: 1)", "b", "2"), "(a: 1, b: 2)");
        assert_eq!(edit("(a: 1,)", "b", "2"), "(a: 1, b: 2,)");
    }

    #[test]
    fn fills_an_empty_or_missing_file() {
        assert_eq!(edit("()", "a", "1"), "(\n    a: 1,\n)");
        assert_eq!(edit("", "a", "1"), "(\n    a: 1,\n)\n");
    }

    #[test]
    fn chars_and_nested_values_do_not_end_a_field() {
        let text = "(\n    a: Some(( code: Char(','), modifiers: \"\")),\n    b: Char('\\''),\n)\n";
        let e = RonEditor::parse(text).unwrap();
        assert_eq!(
            e.get(&["a"]),
            Some(str("Some(( code: Char(','), modifiers: \"\"))"))
        );
        assert_eq!(e.get(&["b"]), Some(str("Char('\\'')")));
    }

    #[test]
    fn root_may_be_named_and_have_attributes() {
        let e = RonEditor::parse("#![enable(implicit_some)]\nTheme(a: 1)").unwrap();
        assert_eq!(e.get(&["a"]), Some(str("1")));
    }

    #[test]
    fn restore_removes_an_added_field_and_collapses_an_empty_struct() {
        for original in ["(\n    a: 1,\n)\n", "()", "(a: 1)", "(a: 1,)"] {
            let mut e = RonEditor::parse(original).unwrap();
            e.set(&["b"], str("2")).unwrap();
            e.restore(&["b"], original).unwrap();
            assert_eq!(e.render(), original);
        }
    }

    #[test]
    fn restore_puts_back_a_replaced_value() {
        let original = "(\n    a: 1,\n)\n";
        let mut e = RonEditor::parse(original).unwrap();
        e.set(&["a"], str("2")).unwrap();
        e.restore(&["a"], original).unwrap();
        assert_eq!(e.render(), original);
    }

    #[test]
    fn rejects_text_that_is_not_a_struct() {
        assert!(RonEditor::parse("[1, 2]").is_err());
        assert!(RonEditor::parse("(a: 1").is_err());
    }

    #[test]
    fn nested_paths_and_lists_are_errors() {
        let mut e = RonEditor::parse("()").unwrap();
        assert!(e.set(&["a", "b"], str("1")).is_err());
        assert!(e.set(&["a"], Value::List(vec![])).is_err());
    }
}
