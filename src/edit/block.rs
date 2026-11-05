use anyhow::{Result, bail};

use super::{Editor, Value};

const START: &str = "# >>> vimify >>>";
const END: &str = "# <<< vimify <<<";

/// A region vimify owns outright, for configs with no key structure to edit.
pub struct BlockEditor {
    before: String,
    block: Option<String>,
    after: String,
}

impl BlockEditor {
    pub fn parse(text: &str) -> Result<Self> {
        let Some((before, rest)) = split_at_line(text, START) else {
            if split_at_line(text, END).is_some() {
                bail!("end marker without a start marker");
            }
            return Ok(Self {
                before: text.to_owned(),
                block: None,
                after: String::new(),
            });
        };
        let Some((block, after)) = split_at_line(rest, END) else {
            bail!("start marker without an end marker");
        };
        Ok(Self {
            before: before.to_owned(),
            block: Some(block.trim_end_matches('\n').to_owned()),
            after: after.to_owned(),
        })
    }
}

/// Text before and after the first line equal to `marker`.
fn split_at_line<'a>(text: &'a str, marker: &str) -> Option<(&'a str, &'a str)> {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let end = offset + line.len();
        if line.trim_end() == marker {
            return Some((&text[..offset], &text[end..]));
        }
        offset = end;
    }
    None
}

impl Editor for BlockEditor {
    fn set(&mut self, _path: &[&str], value: Value) -> Result<()> {
        let Value::Str(text) = value else {
            bail!("a block is text");
        };
        self.block = Some(text.trim_end_matches('\n').to_owned());
        Ok(())
    }

    fn get(&self, _path: &[&str]) -> Option<Value> {
        self.block.clone().map(Value::Str)
    }

    fn restore(&mut self, _path: &[&str], original: &str) -> Result<()> {
        self.block = Self::parse(original)?.block;
        Ok(())
    }

    fn render(&self) -> String {
        let Self { before, after, .. } = self;
        let Some(block) = &self.block else {
            return format!("{before}{after}");
        };
        let newline = if before.is_empty() || before.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        format!("{before}{newline}{START}\n{block}\n{END}\n{after}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn str(s: &str) -> Value {
        Value::Str(s.into())
    }

    fn edit(text: &str, value: &str) -> Result<String> {
        let mut editor = BlockEditor::parse(text)?;
        editor.set(&["block"], str(value))?;
        Ok(editor.render())
    }

    const WITH_BLOCK: &str = "a\n# >>> vimify >>>\nold\n# <<< vimify <<<\nb\n";

    #[test]
    fn appends_a_block_after_existing_text() {
        assert_eq!(
            edit("a\n", "new").unwrap(),
            "a\n# >>> vimify >>>\nnew\n# <<< vimify <<<\n"
        );
    }

    #[test]
    fn an_empty_file_gets_only_the_block() {
        assert_eq!(
            edit("", "new\n").unwrap(),
            "# >>> vimify >>>\nnew\n# <<< vimify <<<\n"
        );
    }

    #[test]
    fn replaces_only_the_region() {
        assert_eq!(
            edit(WITH_BLOCK, "new").unwrap(),
            "a\n# >>> vimify >>>\nnew\n# <<< vimify <<<\nb\n"
        );
    }

    #[test]
    fn get_reads_the_block() {
        let editor = BlockEditor::parse(WITH_BLOCK).unwrap();
        assert_eq!(editor.get(&["block"]), Some(str("old")));
        assert_eq!(BlockEditor::parse("a\n").unwrap().get(&["block"]), None);
    }

    #[test]
    fn restore_removes_the_block_and_keeps_the_rest() {
        let mut editor = BlockEditor::parse(WITH_BLOCK).unwrap();
        editor.restore(&["block"], "").unwrap();
        assert_eq!(editor.render(), "a\nb\n");
    }

    #[test]
    fn restore_brings_back_a_block_the_original_had() {
        let mut editor = BlockEditor::parse(&edit(WITH_BLOCK, "new").unwrap()).unwrap();
        editor.restore(&["block"], WITH_BLOCK).unwrap();
        assert_eq!(editor.render(), WITH_BLOCK);
    }

    #[test]
    fn a_broken_block_is_refused() {
        assert!(BlockEditor::parse("# >>> vimify >>>\nx\n").is_err());
        assert!(BlockEditor::parse("x\n# <<< vimify <<<\n").is_err());
    }

    #[test]
    fn setting_twice_is_idempotent() {
        let once = edit("a\n", "new").unwrap();
        assert_eq!(edit(&once, "new").unwrap(), once);
    }
}
