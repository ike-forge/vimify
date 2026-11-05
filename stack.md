# Stack

| Concern     | Choice                          |
| ----------- | ------------------------------- |
| Language    | Rust (edition 2024)             |
| Toolchain   | mise, pinned in `mise.toml`     |
| UI          | ratatui + crossterm             |
| Own config  | TOML                            |

## Tool config editing

Edits must preserve comments, ordering and quoting of everything vimify does
not own.

| Format             | Approach                                           |
| ------------------ | -------------------------------------------------- |
| TOML               | `toml_edit`                                        |
| YAML               | `yaml-edit` (rowan CST)                            |
| JSON / JSONC       | `jsonc-parser`, `cst` feature                      |
| `key = value`      | Own line editor, values verbatim                   |
| RON                | Own field editor, values verbatim                  |
| Line-based (`txt`) | Owned block between `# >>> vimify >>>` markers     |

Preferred over all of the above: if the tool supports `include`/`import`/
`source` (kitty, tmux, alacritty, git), write a vimify-owned file and add one
include line.

> **Weak point:** `yaml-edit` is a single-maintainer crate with little adoption.
> It passed preservation tests on lazygit-style configs (nested set, key insert,
> flow maps, comments), but it should be replaced once a mature
> format-preserving YAML crate exists. Keep it behind an internal
> `set(path, value)` interface so the swap stays local.

## Why ratatui

Prompt crates (dialoguer, inquire) would cover a progress bar and a
multi-select, but the tool list needs:

- `apply to all` / `apply to none` rows that drive the other rows.
- Per-tool status (applied, not applied, modified since apply).
- hjkl navigation. A tool named vimify cannot ship without it.

All three fight a prompt crate's fixed widget. ratatui keeps the UI one custom
widget plus a progress gauge.

## Dev commands

```sh
mise install     # pinned rust + clippy + rustfmt
mise run build   # release build
mise run run     # run vimify
mise run test
mise run lint    # fmt --check + clippy -D warnings
mise run fmt
mise run sandbox # fresh Debian container with vimify + target tools
mise run e2e     # real vimify and tools in that container, through tmux
```

The sandbox installs release binaries pinned by sha256 in `docker/tools.lock`
(apt for distro-packaged tools). Add a tool there when adding its port. The host
Docker socket is mounted when present, for lazydocker. `mise run
sandbox:cluster` starts a kind cluster for k9s; the sandbox joins it when it
exists. Demo data comes from `docker/seed.sh`; `~` is itself a git repo, so
`lazygit` in `~` shows what vimify changed.

## Interface

```
$ vimify

 Scanning  ███████████░░░░░░░  6/11  ~/.cargo/bin

 [x] apply to all
 [ ] apply to none
 ─────────────────
 [x] lazygit
 [ ] lazysql      (excluded)
 [x] yazi
```

- Progress is per scanned directory, not per file.
- Scan directories: a built-in list (`/usr/bin`, `/usr/local/bin`,
  `/opt/homebrew/bin`, `/home/linuxbrew/.linuxbrew/bin`, `~/.local/bin`,
  `~/.cargo/bin`, mise shims, ...) plus user entries from `scan.extra_paths`.
- A tool is detected when its binary name from the mapping DB is found.
- Unknown binaries are never listed.

## Files

| Path                                | Content                                        |
| ----------------------------------- | ---------------------------------------------- |
| `~/.config/vimify/config.toml`      | Reference config, extra scan paths, exclusions |

Exclusions live in the config, not in a state dir, so a dotfiles checkout
carries them to new machines.

## Open questions

- Whether to add non-interactive subcommands (`vimify apply --all`) for
  scripting. Implies `clap`.
