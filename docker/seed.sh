#!/bin/sh
# Demo data so each tool has something to show.
set -eu
cd "$HOME"

git config --global user.name dev
git config --global user.email dev@localhost
git config --global init.defaultBranch main

# lazygit, gitui: history, a branch, staged-able and untracked changes.
git init -q git-repo
cd git-repo
printf '# demo\n' > README.md
git add . && git commit -qm "Add readme"
mkdir src
printf 'fn main() {\n    println!("hello");\n}\n' > src/main.rs
git add . && git commit -qm "Add main"
git switch -qc feature
printf 'fn main() {\n    println!("hello, feature");\n}\n' > src/main.rs
git commit -qam "Greet from feature"
git switch -q main
printf '\nWork in progress.\n' >> README.md
printf 'scratch\n' > notes.txt
cd "$HOME"

# lazysql. Its config doubles as pre-existing user config vimify must preserve.
sqlite3 demo.db "create table users (id integer primary key, name text);
  insert into users (name) values ('ada'), ('linus'), ('grace');"
mkdir -p .config/lazysql
cat > .config/lazysql/config.toml <<'TOML'
[[database]]
Name = 'demo'
Provider = 'sqlite3'
URL = '/home/dev/demo.db'
TOML

# bat: `bat README.md` shows the theme on Markdown and embedded code.
cat > README.md <<'MD'
# Sandbox

A demo file for **bat**. Run `bat README.md`, then compare with `bat --theme=Monokai\ Extended README.md`.

## Lists and links

- *Emphasis*, **strong**, ~~struck~~ and `inline code`
- [vimify](https://github.com/ike-forge/vimify)
  1. Nested item
  2. Another one

> A quote, with a [link](https://example.com).

## Code

```rust
use std::collections::HashMap;

/// Counts words, case-insensitively.
fn count(text: &str) -> HashMap<String, usize> {
    let mut words = HashMap::new();
    for w in text.split_whitespace() {
        *words.entry(w.to_lowercase()).or_insert(0) += 1;
    }
    words
}
```

```sh
for f in ~/.config/*/config*; do
  [ -f "$f" ] && echo "found: $f" || echo "none" >&2
done
```

```toml
[keys]
up = "k"
down = "j"
enabled = true
retries = 3
```

| Tool | Format |
|------|--------|
| bat  | lines  |
| k9s  | yaml   |
MD

# Home as a dotfiles repo: `lazygit` in ~ shows what vimify changed.
printf '/*\n!/.config/\n!/.gitignore\n' > .gitignore
git init -q
git add .
git commit -qm "Initial dotfiles"
