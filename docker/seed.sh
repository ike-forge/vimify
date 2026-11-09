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

## Filler

Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor
incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis
nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.

Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu
fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in
culpa qui officia deserunt mollit anim id est laborum.

Curabitur pretium tincidunt lacus. Nulla gravida orci a odio. Nullam varius,
turpis et commodo pharetra, est eros bibendum elit, nec luctus magna felis
sollicitudin mauris. Integer in mauris eu nibh euismod gravida.

Duis ac tellus et risus vulputate vehicula. Donec lobortis risus a elit. Etiam
tempor. Ut ullamcorper, ligula eu tempor congue, eros est euismod turpis, id
tincidunt sapien risus a quam. Maecenas fermentum consequat mi.

Donec fermentum. Pellentesque malesuada nulla a mi. Duis sapien sem, aliquet
nec, commodo eget, consequat quis, neque. Aliquam faucibus, elit ut dictum
aliquet, felis nisl adipiscing sapien, sed malesuada diam lacus eget erat.

Cras mollis scelerisque nunc. Nullam arcu. Aliquam consequat. Curabitur augue
lorem, dapibus quis, laoreet et, pretium ac, nisi. Aenean magna nisl, mollis
quis, molestie eu, feugiat in, orci. In hac habitasse platea dictumst.

Fusce convallis, mauris imperdiet gravida bibendum, nisl turpis suscipit
mauris, sed placerat ipsum urna sed risus. In convallis tellus a mauris.
Curabitur non elit ut libero tristique sodales. Mauris a lacus. Donec mattis
semper leo. In hac habitasse platea dictumst.

Vivamus facilisis diam at odio. Mauris dictum, nisi eget consequat elementum,
lacus ligula molestie metus, non feugiat orci magna ac sem. Donec turpis.
Donec vitae metus. Morbi tristique neque eu mauris. Quisque gravida ipsum non
sapien. Proin turpis lacus, scelerisque vitae, elementum at, lobortis ac, quam.

Aliquam dictum eleifend risus. In hac habitasse platea dictumst. Etiam sit amet
diam. Suspendisse odio. Suspendisse nunc. In semper bibendum libero. Proin
nonummy, lacus eget pulvinar lacinia, pede felis dignissim leo, vitae
tristique magna lacus sit amet eros.

Nullam ornare. Praesent odio ligula, dapibus sed, tincidunt eget, dictum ac,
nibh. Nam quis lacus. Nunc eleifend molestie velit. Morbi lobortis quam eu
velit. Donec euismod vestibulum massa. Donec non lectus. Aliquam commodo lacus
sit amet nulla. Cras dignissim elit et augue. Nullam non diam.
MD

# Home as a dotfiles repo: `lazygit` in ~ shows what vimify changed.
printf '/*\n!/.config/\n!/.gitignore\n' > .gitignore
git init -q
git add .
git commit -qm "Initial dotfiles"
