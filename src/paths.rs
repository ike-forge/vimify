use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Expands a leading `~` and `$VAR` segments. `None` if a variable is unset.
pub fn expand(raw: &str) -> Option<PathBuf> {
    expand_with(raw, std::env::home_dir().as_deref(), |name| {
        std::env::var_os(name)
    })
}

/// Inverse of `~` expansion, for display.
pub fn display(path: &Path) -> String {
    if let Some(home) = std::env::home_dir()
        && let Ok(rest) = path.strip_prefix(&home)
    {
        return format!("~/{}", rest.display());
    }
    path.display().to_string()
}

fn expand_with(
    raw: &str,
    home: Option<&Path>,
    var: impl Fn(&str) -> Option<OsString>,
) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for (i, seg) in raw.split('/').enumerate() {
        match seg {
            "" if i == 0 => out.push("/"),
            "~" if i == 0 => out.push(home?),
            _ => match seg.strip_prefix('$') {
                Some(name) => out.push(lookup(name, home, &var)?),
                None => out.push(seg),
            },
        }
    }
    Some(out)
}

// XDG base dirs fall back to their spec defaults when unset.
fn lookup(
    name: &str,
    home: Option<&Path>,
    var: &impl Fn(&str) -> Option<OsString>,
) -> Option<PathBuf> {
    if let Some(value) = var(name).filter(|v| !v.is_empty()) {
        return Some(value.into());
    }
    let fallback = match name {
        "XDG_CONFIG_HOME" => ".config",
        "XDG_DATA_HOME" => ".local/share",
        "XDG_STATE_HOME" => ".local/state",
        _ => return None,
    };
    Some(home?.join(fallback))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expand(raw: &str) -> Option<PathBuf> {
        expand_with(raw, Some(Path::new("/home/u")), |name| match name {
            "SET" => Some("/set".into()),
            "EMPTY" => Some("".into()),
            _ => None,
        })
    }

    #[test]
    fn absolute_path_unchanged() {
        assert_eq!(expand("/usr/bin"), Some("/usr/bin".into()));
    }

    #[test]
    fn tilde_is_home() {
        assert_eq!(expand("~/.cargo/bin"), Some("/home/u/.cargo/bin".into()));
    }

    #[test]
    fn var_is_substituted() {
        assert_eq!(expand("$SET/x"), Some("/set/x".into()));
    }

    #[test]
    fn unset_or_empty_var_is_none() {
        assert_eq!(expand("$UNSET/x"), None);
        assert_eq!(expand("$EMPTY/x"), None);
    }

    #[test]
    fn xdg_falls_back_to_default() {
        assert_eq!(
            expand("$XDG_DATA_HOME/mise/shims"),
            Some("/home/u/.local/share/mise/shims".into())
        );
    }
}
