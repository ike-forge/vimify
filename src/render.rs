use anyhow::{Context, Result};

use crate::edit::{self, Value};
use crate::palette::{self, Palette};
use crate::port::{Target, ThemeValue};
use crate::reference::Reference;

#[derive(Debug, PartialEq, Eq)]
pub struct Rendered {
    pub values: Vec<(String, Value)>,
    pub warnings: Vec<String>,
}

pub fn render(
    tool: &str,
    target: &Target,
    reference: &Reference,
    palette: &Palette,
) -> Result<Rendered> {
    let mut values = Vec::new();
    let mut warnings = Vec::new();
    for (setting, action) in &target.keys {
        let key = reference
            .keys
            .get(action)
            .with_context(|| format!("no key for action {action}"))?;
        match target.key_notation.and_then(|n| n.convert(key)) {
            Some(converted) => values.push((setting.clone(), Value::Str(converted))),
            None => warnings.push(format!("{tool} cannot express key {key}")),
        }
    }
    for (setting, value) in &target.theme {
        let value = match value {
            ThemeValue::One(s) => Value::Str(fill(s, palette)?),
            ThemeValue::Many(items) => Value::List(
                items
                    .iter()
                    .map(|s| fill(s, palette))
                    .collect::<Result<_>>()?,
            ),
        };
        values.push((setting.clone(), value));
    }
    if let Some(template) = &target.template {
        values.push((edit::BLOCK.to_owned(), Value::Str(fill(template, palette)?)));
    }
    Ok(Rendered { values, warnings })
}

fn fill(value: &str, palette: &Palette) -> Result<String> {
    let mut filled = value.to_owned();
    for role in palette::placeholders(value) {
        let color = palette
            .role(role)
            .with_context(|| format!("no colour for role {role}"))?;
        filled = filled.replace(&format!("{{{role}}}"), color);
    }
    Ok(filled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::port;

    fn render_port(text: &str, reference: &Reference) -> Rendered {
        let port = port::parse("t", text).unwrap();
        let palette = palette::load("catppuccin-frappe").unwrap();
        render(&port.name, &port.targets[0], reference, &palette).unwrap()
    }

    const TARGET: &str = "schema = 1\nversion = \"1\"\nbinaries = []\n[[target]]\nformat = \"yaml\"\nstrategy = \"edit\"\nkey_notation = \"angle\"\npaths = []\n";

    #[test]
    fn keys_go_through_the_notation() {
        let mut reference = Reference::default();
        reference.keys.insert("down".into(), "<C-n>".into());
        let text = format!("{TARGET}[target.keys]\n\"a.b\" = \"down\"\n");

        let out = render_port(&text, &reference);

        assert_eq!(out.values, [("a.b".into(), Value::Str("<c-n>".into()))]);
    }

    #[test]
    fn inexpressible_key_is_skipped_with_a_warning() {
        let mut reference = Reference::default();
        reference.keys.insert("down".into(), "gg".into());
        let text = format!("{TARGET}[target.keys]\n\"a.b\" = \"down\"\n");

        let out = render_port(&text, &reference);

        assert!(out.values.is_empty());
        assert_eq!(out.warnings, ["t cannot express key gg"]);
    }

    #[test]
    fn theme_fills_roles_and_keeps_literals() {
        let text = format!(
            "{TARGET}[target.theme]\n\"a.b\" = [\"{{accent}}\", \"bold\"]\nc = \"{{selection}}\"\n"
        );

        let out = render_port(&text, &Reference::default());

        assert_eq!(
            out.values,
            [
                (
                    "a.b".into(),
                    Value::List(vec!["#8caaee".into(), "bold".into()])
                ),
                ("c".into(), Value::Str("#414559".into())),
            ]
        );
    }
    #[test]
    fn roles_fill_inside_text() {
        let text = format!("{TARGET}[target.theme]\na = \"\\\"{{accent}}\\\"\"\n");

        let out = render_port(&text, &Reference::default());

        assert_eq!(out.values, [("a".into(), Value::Str("\"#8caaee\"".into()))]);
    }

    #[test]
    fn a_template_renders_as_the_block() {
        let text = TARGET
            .replace("\"edit\"", "\"block\"")
            .replace("\"yaml\"", "\"lines\"");
        let text = format!("{text}template = \"bg {{base}}\"\n");

        let out = render_port(&text, &Reference::default());

        assert_eq!(
            out.values,
            [(edit::BLOCK.into(), Value::Str("bg #303446".into()))]
        );
    }
}
