use anyhow::{Context, Result};

use crate::edit::Value;
use crate::palette::{self, Palette};
use crate::port::{Port, ThemeValue};
use crate::reference::Reference;

#[derive(Debug, PartialEq, Eq)]
pub struct Rendered {
    pub values: Vec<(String, Value)>,
    pub warnings: Vec<String>,
}

pub fn render(port: &Port, reference: &Reference, palette: &Palette) -> Result<Rendered> {
    let mut values = Vec::new();
    let mut warnings = Vec::new();
    let notation = port.target.as_ref().and_then(|t| t.key_notation);
    for (setting, action) in &port.keys {
        let key = reference
            .keys
            .get(action)
            .with_context(|| format!("no key for action {action}"))?;
        match notation.and_then(|n| n.convert(key)) {
            Some(converted) => values.push((setting.clone(), Value::Str(converted))),
            None => warnings.push(format!("{} cannot express key {key}", port.name)),
        }
    }
    for (setting, value) in &port.theme {
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
    Ok(Rendered { values, warnings })
}

fn fill(value: &str, palette: &Palette) -> Result<String> {
    match palette::role_of(value) {
        Some(role) => palette
            .role(role)
            .map(str::to_owned)
            .with_context(|| format!("no colour for role {role}")),
        None => Ok(value.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::port;

    fn render_port(text: &str, reference: &Reference) -> Rendered {
        let port = port::parse("t", text).unwrap();
        let palette = palette::load("catppuccin-frappe").unwrap();
        render(&port, reference, &palette).unwrap()
    }

    const TARGET: &str = "schema = 1\nversion = \"1\"\nbinaries = []\n[target]\nformat = \"yaml\"\nstrategy = \"edit\"\nkey_notation = \"angle\"\npaths = []\n";

    #[test]
    fn keys_go_through_the_notation() {
        let mut reference = Reference::default();
        reference.keys.insert("down".into(), "<C-n>".into());
        let text = format!("{TARGET}[keys]\n\"a.b\" = \"down\"\n");

        let out = render_port(&text, &reference);

        assert_eq!(out.values, [("a.b".into(), Value::Str("<c-n>".into()))]);
    }

    #[test]
    fn inexpressible_key_is_skipped_with_a_warning() {
        let mut reference = Reference::default();
        reference.keys.insert("down".into(), "gg".into());
        let text = format!("{TARGET}[keys]\n\"a.b\" = \"down\"\n");

        let out = render_port(&text, &reference);

        assert!(out.values.is_empty());
        assert_eq!(out.warnings, ["t cannot express key gg"]);
    }

    #[test]
    fn theme_fills_roles_and_keeps_literals() {
        let text = format!(
            "{TARGET}[theme]\n\"a.b\" = [\"{{accent}}\", \"bold\"]\nc = \"{{selection}}\"\n"
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
}
