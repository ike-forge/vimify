# Stack

| Concern     | Choice                          |
| ----------- | ------------------------------- |
| Language    | Rust (edition 2024)             |
| Toolchain   | mise, pinned in `mise.toml`     |
| UI          | ratatui + crossterm             |
| Own config  | TOML                            |

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
```

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

- Format-preserving edits for non-TOML tool configs. `toml_edit` covers TOML;
  YAML has no mature format-preserving crate in Rust, and many TUI tools
  (lazygit, lazysql, k9s) use YAML.
- Whether to add non-interactive subcommands (`vimify apply --all`) for
  scripting. Implies `clap`.
