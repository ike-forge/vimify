use anyhow::{Context, Result, anyhow, bail};
use toml_edit::{Array, DocumentMut, InlineTable, Item, Table, TableLike};

use super::{Editor, Value};

pub struct TomlEditor {
    doc: DocumentMut,
    /// A new table starts with a blank line, which an empty file should not open with.
    empty: bool,
}

impl TomlEditor {
    pub fn parse(text: &str) -> Result<Self> {
        Ok(Self {
            doc: text.parse().map_err(|e| anyhow!("{e}"))?,
            empty: text.trim().is_empty(),
        })
    }
}

impl Editor for TomlEditor {
    fn set(&mut self, path: &[&str], value: Value) -> Result<()> {
        set_in(self.doc.as_table_mut(), path, &value).with_context(|| path.join("."))
    }

    fn get(&self, path: &[&str]) -> Option<Value> {
        let (last, parents) = path.split_last()?;
        let mut table: &dyn TableLike = self.doc.as_table();
        for (i, key) in parents.iter().enumerate() {
            if let Some(entry) = Entry::parse(key) {
                let inline = entry.find(table.get(entry.array)?.as_array()?)?;
                return inline
                    .get(parents.get(i + 1).unwrap_or(last))
                    .map(string)
                    .map(Value::Str);
            }
            table = table.get(key)?.as_table_like()?;
        }
        table.get(last).map(from_toml)
    }

    fn restore(&mut self, path: &[&str], original: &str) -> Result<()> {
        let original = Self::parse(original)?;
        restore_in(self.doc.as_table_mut(), Some(original.doc.as_table()), path);
        Ok(())
    }

    fn render(&self) -> String {
        let text = self.doc.to_string();
        if self.empty {
            return text.trim_start().to_owned();
        }
        text
    }
}

/// `keymap[run=quit]`: the entry of the `keymap` array whose `run` is `quit`, for tools
/// that key a list of tables by one of their fields.
struct Entry<'a> {
    array: &'a str,
    field: &'a str,
    value: &'a str,
}

impl<'a> Entry<'a> {
    fn parse(segment: &'a str) -> Option<Self> {
        let (array, selector) = segment.strip_suffix(']')?.split_once('[')?;
        let (field, value) = selector.split_once('=')?;
        Some(Self {
            array,
            field,
            value,
        })
    }

    fn position(&self, array: &Array) -> Option<usize> {
        array.iter().position(|item| {
            let field = item.as_inline_table().and_then(|t| t.get(self.field));
            field.and_then(toml_edit::Value::as_str) == Some(self.value)
        })
    }

    fn find<'b>(&self, array: &'b Array) -> Option<&'b InlineTable> {
        array.get(self.position(array)?)?.as_inline_table()
    }
}

fn set_entry(table: &mut dyn TableLike, entry: &Entry, field: &str, value: &Value) -> Result<()> {
    if !table.contains_key(entry.array) {
        table.insert(entry.array, Item::Value(Array::new().into()));
    }
    let Some(array) = table.get_mut(entry.array).and_then(Item::as_array_mut) else {
        bail!("{} is not an inline array", entry.array);
    };
    let position = entry.position(array).unwrap_or_else(|| {
        // A multi-line array stays one: `push` would put the entry on the last line.
        let last = array.iter().last().and_then(|item| item.decor().prefix());
        let indent = last
            .and_then(|prefix| prefix.as_str()?.rsplit_once('\n'))
            .map(|(_, indent)| format!("\n{indent}"));
        let mut new = InlineTable::new();
        new.insert(entry.field, entry.value.into());
        array.push(new);
        if let (Some(indent), Some(item)) = (indent, array.iter_mut().last()) {
            item.decor_mut().set_prefix(indent);
        }
        array.len() - 1
    });
    let item = array
        .get_mut(position)
        .and_then(toml_edit::Value::as_inline_table_mut);
    let Some(item) = item else {
        bail!("{} is not an inline table", entry.array);
    };
    item.insert(field, to_toml(value));
    Ok(())
}

/// Drops the entry and an array only it kept alive, or brings the original's field back.
fn restore_entry(
    table: &mut dyn TableLike,
    original: Option<&dyn TableLike>,
    entry: &Entry,
    field: &str,
) {
    let Some(array) = table.get_mut(entry.array).and_then(Item::as_array_mut) else {
        return;
    };
    let source = original
        .and_then(|t| t.get(entry.array))
        .and_then(Item::as_array);
    let kept = source.and_then(|a| entry.find(a));
    let Some(position) = entry.position(array) else {
        return;
    };
    match kept.and_then(|t| t.get(field)) {
        Some(value) => {
            if let Some(item) = array
                .get_mut(position)
                .and_then(toml_edit::Value::as_inline_table_mut)
            {
                item.insert(field, value.clone());
            }
        }
        None => {
            array.remove(position);
        }
    }
    if array.is_empty() && source.is_none() {
        table.remove(entry.array);
    }
}

fn set_in(table: &mut dyn TableLike, path: &[&str], value: &Value) -> Result<()> {
    let [key, rest @ ..] = path else {
        bail!("empty path");
    };
    if let (Some(entry), [field]) = (Entry::parse(key), rest) {
        return set_entry(table, &entry, field, value);
    }
    if rest.is_empty() {
        let mut new = to_toml(value);
        // In place: `insert` replaces the key too, which holds the comment above it.
        match table.get_mut(key) {
            Some(item) => {
                if let Some(old) = item.as_value() {
                    *new.decor_mut() = old.decor().clone();
                }
                *item = Item::Value(new);
            }
            None => {
                table.insert(key, Item::Value(new));
            }
        }
        return Ok(());
    }
    if !table.contains_key(key) {
        table.insert(key, Item::Table(new_table()));
    }
    match table.get_mut(key).and_then(Item::as_table_like_mut) {
        Some(child) => set_in(child, rest, value),
        // Not vimify's value to replace.
        None => bail!("{key} is not a table"),
    }
}

/// Implicit, so `[keymap.Tree]` is not preceded by an empty `[keymap]`.
fn new_table() -> Table {
    let mut table = Table::new();
    table.set_implicit(true);
    table.decor_mut().set_prefix("\n");
    table
}

/// A parent the user replaced or removed is left as it is.
fn restore_in(current: &mut dyn TableLike, original: Option<&dyn TableLike>, path: &[&str]) {
    let [key, rest @ ..] = path else {
        return;
    };
    if let (Some(entry), [field]) = (Entry::parse(key), rest) {
        restore_entry(current, original, &entry, field);
        return;
    }
    if !rest.is_empty() {
        let Some(child) = current.get_mut(key).and_then(Item::as_table_like_mut) else {
            return;
        };
        let original_child = original
            .and_then(|table| table.get(key))
            .and_then(Item::as_table_like);
        restore_in(child, original_child, rest);
        if !child.is_empty() {
            return;
        }
    }
    match original.and_then(|table| table.get(key)) {
        Some(item) => {
            current.insert(key, item.clone());
        }
        None => {
            current.remove(key);
        }
    }
}

/// Anything but strings is kept as text, which compares unequal to what vimify wrote.
fn from_toml(item: &Item) -> Value {
    if let Some(array) = item.as_array() {
        return Value::List(array.iter().map(string).collect());
    }
    match item.as_str() {
        Some(s) => Value::Str(s.to_owned()),
        None => Value::Str(item.to_string()),
    }
}

fn string(value: &toml_edit::Value) -> String {
    match value.as_str() {
        Some(s) => s.to_owned(),
        None => value.to_string(),
    }
}

fn to_toml(value: &Value) -> toml_edit::Value {
    match value {
        Value::Str(s) => s.as_str().into(),
        Value::List(items) => items.iter().map(String::as_str).collect::<Array>().into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(text: &str, path: &[&str], value: Value) -> Result<String> {
        let mut editor = TomlEditor::parse(text)?;
        editor.set(path, value)?;
        Ok(editor.render())
    }

    fn str(s: &str) -> Value {
        Value::Str(s.into())
    }

    fn list(items: &[&str]) -> Value {
        Value::List(items.iter().map(|s| s.to_string()).collect())
    }

    fn restore(text: &str, path: &[&str], original: &str) -> String {
        let mut editor = TomlEditor::parse(text).unwrap();
        editor.restore(path, original).unwrap();
        editor.render()
    }

    fn get(text: &str, path: &[&str]) -> Option<Value> {
        TomlEditor::parse(text).unwrap().get(path)
    }

    #[test]
    fn replaces_value_and_keeps_comments_and_order() {
        let text = "# top\n[a]\n# mine\nb = 'old' # why\nc = 1 # keep\n\n[d]\ne = 2\n";
        let out = edit(text, &["a", "b"], str("new")).unwrap();
        assert_eq!(
            out,
            "# top\n[a]\n# mine\nb = \"new\" # why\nc = 1 # keep\n\n[d]\ne = 2\n"
        );
    }

    #[test]
    fn creates_missing_tables_after_array_tables() {
        let text = "[[database]]\nName = 'demo'\n";
        let out = edit(text, &["keymap", "Tree", "MoveDown"], str("j")).unwrap();
        assert_eq!(
            out,
            "[[database]]\nName = 'demo'\n\n[keymap.Tree]\nMoveDown = \"j\"\n"
        );
    }

    #[test]
    fn inserts_into_existing_table() {
        let out = edit("[a]\nb = 1\n", &["a", "c"], str("x")).unwrap();
        assert_eq!(out, "[a]\nb = 1\nc = \"x\"\n");
    }

    #[test]
    fn adds_to_an_implicit_parent() {
        let out = edit("[a.b]\nc = 1\n", &["a", "d", "e"], str("x")).unwrap();
        assert_eq!(out, "[a.b]\nc = 1\n\n[a.d]\ne = \"x\"\n");
    }

    #[test]
    fn empty_file_gets_content() {
        let out = edit("", &["a", "b"], str("x")).unwrap();
        assert_eq!(out, "[a]\nb = \"x\"\n");
    }

    #[test]
    fn inline_table_stays_inline() {
        let out = edit("a = { b = 1 }\n", &["a", "b"], str("x")).unwrap();
        assert_eq!(out, "a = { b = \"x\" }\n");
    }

    #[test]
    fn scalar_parent_is_refused() {
        let err = edit("a = 1\n", &["a", "b"], str("x")).unwrap_err();
        assert!(format!("{err:#}").contains("a is not a table"));
    }

    #[test]
    fn array_of_tables_parent_is_refused() {
        assert!(edit("[[a]]\nb = 1\n", &["a", "b"], str("x")).is_err());
    }

    #[test]
    fn invalid_toml_is_refused() {
        assert!(TomlEditor::parse("a = [").is_err());
    }

    #[test]
    fn writes_lists() {
        let out = edit("", &["a"], list(&["x", "y"])).unwrap();
        assert_eq!(out, "a = [\"x\", \"y\"]\n");
    }

    #[test]
    fn get_reads_written_values_however_they_are_quoted() {
        let text = "[a]\nb = 'j'\nc = \"#8caaee\"\nd = ['x', \"y\"]\n";
        assert_eq!(get(text, &["a", "b"]), Some(str("j")));
        assert_eq!(get(text, &["a", "c"]), Some(str("#8caaee")));
        assert_eq!(get(text, &["a", "d"]), Some(list(&["x", "y"])));
    }

    #[test]
    fn get_of_an_absent_path_is_none() {
        assert_eq!(get("a = 1\n", &["b"]), None);
        assert_eq!(get("a = 1\n", &["a", "b"]), None);
    }

    #[test]
    fn get_of_a_table_differs_from_any_written_value() {
        let value = get("[a]\nb = 1\n", &["a"]).unwrap();
        assert_ne!(value, str(""));
        assert_ne!(value, list(&[]));
    }

    #[test]
    fn restore_brings_back_the_original_with_type_and_comments() {
        let original = "[a]\nn = 5 # five\ns = ['red'] # r\n";
        let text = "# top\n[a]\nn = \"x\"\ns = \"y\"\nmine = 1 # keep\n";

        let out = ["n", "s"].iter().fold(text.to_owned(), |text, key| {
            restore(&text, &["a", key], original)
        });

        assert_eq!(
            out,
            "# top\n[a]\nn = 5 # five\ns = ['red'] # r\nmine = 1 # keep\n"
        );
    }

    #[test]
    fn restore_removes_a_key_the_original_lacks() {
        let out = restore("[a]\nb = 1\nc = 2 # keep\n", &["a", "b"], "[a]\nc = 0\n");
        assert_eq!(out, "[a]\nc = 2 # keep\n");
    }

    #[test]
    fn restore_prunes_tables_the_original_lacks() {
        let text = "x = 1\n\n[keymap.Tree]\nMoveDown = \"j\"\n\n[y]\nz = 2\n";
        let out = restore(text, &["keymap", "Tree", "MoveDown"], "x = 0\n");
        assert_eq!(out, "x = 1\n\n[y]\nz = 2\n");
    }

    #[test]
    fn restore_keeps_a_sibling_table() {
        let text = "x = 1\n\n[keymap.Tree]\nMoveDown = \"j\"\n\n[keymap.Home]\nQuit = \"x\"\n";
        let out = restore(text, &["keymap", "Tree", "MoveDown"], "");
        assert_eq!(out, "x = 1\n\n[keymap.Home]\nQuit = \"x\"\n");
    }

    #[test]
    fn restore_keeps_a_table_the_user_added_to() {
        let out = restore("[a]\nb = 1\nmine = 2\n", &["a", "b"], "");
        assert_eq!(out, "[a]\nmine = 2\n");
    }

    #[test]
    fn restore_leaves_a_replaced_parent_alone() {
        assert_eq!(
            restore("a = 'mine'\n", &["a", "b"], "[a]\nb = 1\n"),
            "a = 'mine'\n"
        );
    }

    #[test]
    fn setting_twice_is_idempotent() {
        let once = edit("x = 1\n", &["a", "b"], list(&["y"])).unwrap();
        let twice = edit(&once, &["a", "b"], list(&["y"])).unwrap();
        assert_eq!(once, twice);
    }

    const ENTRY: &[&str] = &["mgr", "prepend_keymap[run=arrow next]", "on"];

    #[test]
    fn an_entry_is_added_once_and_then_updated() {
        let once = edit("", ENTRY, str("j")).unwrap();
        assert_eq!(
            once,
            "[mgr]\nprepend_keymap = [{ run = \"arrow next\", on = \"j\" }]\n"
        );
        let twice = edit(&once, ENTRY, str("n")).unwrap();
        assert_eq!(
            twice,
            "[mgr]\nprepend_keymap = [{ run = \"arrow next\", on = \"n\" }]\n"
        );
    }

    #[test]
    fn an_entry_joins_the_users_entries() {
        let text = "[mgr]\nprepend_keymap = [{ on = \"x\", run = \"mine\" }]\n";
        let out = edit(text, ENTRY, str("j")).unwrap();
        assert_eq!(get(&out, ENTRY), Some(str("j")));
        assert!(out.contains("run = \"mine\""));
    }

    #[test]
    fn a_multi_line_array_stays_multi_line() {
        let text = "[mgr]\nprepend_keymap = [\n\t# why\n\t{ on = \"x\", run = \"mine\" },\n]\n";
        let out = edit(text, ENTRY, str("j")).unwrap();
        assert_eq!(
            out,
            "[mgr]\nprepend_keymap = [\n\t# why\n\t{ on = \"x\", run = \"mine\" },\n\t{ run = \"arrow next\", on = \"j\" },\n]\n"
        );
        assert_eq!(restore(&out, ENTRY, text), text);
    }

    #[test]
    fn get_of_an_absent_entry_is_none() {
        let text = "[mgr]\nprepend_keymap = [{ on = \"x\", run = \"mine\" }]\n";
        assert_eq!(get(text, ENTRY), None);
        assert_eq!(get("", ENTRY), None);
    }

    #[test]
    fn restore_removes_an_entry_and_the_array_it_needed() {
        let text = edit("[mgr]\nx = 1\n", ENTRY, str("j")).unwrap();
        assert_eq!(restore(&text, ENTRY, "[mgr]\nx = 1\n"), "[mgr]\nx = 1\n");
    }

    #[test]
    fn restore_keeps_the_users_entries() {
        let original = "[mgr]\nprepend_keymap = [{ on = \"x\", run = \"mine\" }]\n";
        let text = edit(original, ENTRY, str("j")).unwrap();
        let out = restore(&text, ENTRY, original);
        assert_eq!(
            get(&out, &["mgr", "prepend_keymap[run=mine]", "on"]),
            Some(str("x"))
        );
        assert_eq!(get(&out, ENTRY), None);
    }

    #[test]
    fn restore_brings_back_the_key_of_an_entry() {
        let original = "[mgr]\nprepend_keymap = [{ on = \"x\", run = \"arrow next\" }]\n";
        let text = edit(original, ENTRY, str("j")).unwrap();
        assert_eq!(get(&restore(&text, ENTRY, original), ENTRY), Some(str("x")));
    }

    #[test]
    fn an_array_of_tables_is_refused() {
        let text = "[[mgr.prepend_keymap]]\non = \"x\"\nrun = \"mine\"\n";
        assert!(edit(text, ENTRY, str("j")).is_err());
    }
}
