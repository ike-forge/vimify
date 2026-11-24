# Architecture

## Concepts

| Term          | Meaning                                                              |
| ------------- | -------------------------------------------------------------------- |
| Reference     | User's `~/.config/vimify/config.toml`: theme, accent, keys, scan paths, tools. Unset theme and actions take built-in defaults |
| Action        | A vimify-defined verb (`down`, `yank`, ...) bound to a key in the reference |
| Role          | A semantic colour slot (`accent`, `selection`, ...) filled by a palette |
| Palette       | Role → hex map for one theme (`catppuccin-frappe`). Ships with vimify. A palette with `[accents]` lets the reference pick its `accent` by name |
| Theme source  | Where a port's theme comes from: `builtin`, `upstream` or `palette` |
| Port          | Data file describing how one tool maps actions and roles to its config |
| Plan          | List of file operations computed from reference + ports + state; shown before anything is written |
| State         | What vimify applied, per tool. Drives status and revert              |

## Commands

| Command           | Does                                                              |
| ----------------- | ----------------------------------------------------------------- |
| `vimify`          | Prints help                                                       |
| `vimify config`   | Edits theme and keys. Saves only; applied tools plan as `update` on the next apply. Reset to defaults is an unsaved edit; saving removes defaulted settings from the file, so later default changes reach the user |
| `vimify apply`    | The flow below                                                    |
| `vimify apply -y` | No UI: scan, take the select screen's initial checks, execute. Fails without a config file, on an invalid one, or without `tools`. An unknown tool in `tools` is invalid. Leaves `tools` as saved |
| `vimify rollback` | Lists applied tools, all unchecked; checked ones revert through confirm and execute, and leave `tools`. `"all"` narrows to the applied tools that stay. No scan: revert needs only state |

## Flow

```
scan ──▶ select ──▶ plan ──▶ confirm ──▶ execute ──▶ result
```

1. **scan:** walk scan dirs, match binaries against port `binaries`. One progress
   tick per dir. Runs on a worker thread, events over a channel to the UI loop.
2. **select:** `Apply changes`, then a choice between `apply everywhere` and
   `select manually`, `select none`, then detected tools with status (`applied`,
   `modified`, `update`). Enter on `Apply changes` goes to confirm; on any other
   row it toggles. `apply everywhere` checks every supported tool and greys out
   the rows below the choice, which then do nothing. `select manually` keeps the
   checks and makes them editable. `apply everywhere` starts selected on a first
   run and with `tools = "all"`; otherwise `select manually` starts with the
   tools in `tools` checked. Unsupported tools are greyed out at
   the bottom and cannot be checked. `i` opens a modal with what the port covers.
3. **plan:** checkboxes are desired state, diffed against state:

   | Checked | Applied | Op                         |
   | ------- | ------- | -------------------------- |
   | yes     | no      | apply                      |
   | yes     | yes, older port version or changed reference | re-apply |
   | no      | yes     | revert                     |
   | no      | no      | none                       |

   `apply everywhere` saves `tools = "all"`, which also covers tools installed or
   supported later. `select manually` saves the checked tools; listed tools not
   detected this run stay listed. Either is what lets `apply -y` run.
4. **confirm:** the override warning, listing every file to apply and revert.
5. **execute:** per tool, independently. One failure does not stop the others.

## Ports

Mappings go from tool setting to vimify action/role, so a port only covers what
the tool supports. Ports cannot define new actions or roles. A binding takes a
slot the tool offers for alternates where it has one, so the tool's own keys
keep working.

A port with no editor or no mappings yet is `unsupported`. `notes` in `port.toml`
explain what a port leaves out, and appear in the info modal.

```
ports/lazygit/
  port.toml
  tests/
    default.input.yml
    default.expected.yml
```

A port is a list of targets, one per file. Each holds its own settings, so a config
split across files works as different keys of one file does: same plan, state, backup
and revert. A tool applies whole. Every file is computed first, and a failed write
puts back the ones already written.

```toml
schema = 1
version = "0.1.0"
binaries = ["lazygit"]

[[target]]
format = "yaml"            # toml | yaml | ron | lines | args | xml
strategy = "edit"          # edit | block | file | include
key_notation = "angle"     # how the tool spells <C-u>
paths = [                  # first existing wins; first resolvable is created
  "$LG_CONFIG_FILE",
  "$XDG_CONFIG_HOME/lazygit/config.yml",
  "~/Library/Application Support/lazygit/config.yml",
]

# The -alt slots: the primary ones hold the arrow keys, which must keep working.
[target.keys]
"keybinding.universal.prevItem-alt" = "up"
"keybinding.universal.nextItem-alt" = "down"
"keybinding.universal.prevBlock-alt" = "left"
"keybinding.universal.nextBlock-alt" = "right"

[target.theme]
"gui.theme.activeBorderColor" = ["{accent}", "bold"]
"gui.theme.selectedLineBgColor" = ["{selection}"]
```

`{role}` fills anywhere in a value (`'"{accent}"'`), for tools that want it quoted.
A TOML `true` or `false` stays a bool, for YAML and TOML fields that refuse the
string; other formats reject it.
Golden tests pick their target by file stem: `theme.input.toml` tests the target
writing `theme.toml`. Any other stem tests the first target.

TOML settings reach into arrays of inline tables by a field of the entry:
`"mgr.prepend_keymap[run=arrow next].on" = "down"`. A missing entry is appended, after
the user's, and revert takes it back out.

`lines` targets edit `key = value` lines, `args` targets command-line flags
(`--theme="ansi"`, the last of a repeated flag), both with values verbatim. A `block`
target is one template instead of settings, for configs where a setting is not one
line:

```toml
[[target]]
format = "lines"
strategy = "block"
paths = ["$XDG_CONFIG_HOME/kitty/kitty.conf"]
template = "background {base}"
```

A port that changes a target's format or strategy takes out what the old one wrote
before applying, with the editor state recorded.

A block target with keys writes one `key_line` per key after the template, for configs
that bind as `key action` lines:

```toml
template = "#command"
key_line = "{key} {setting}"

[target.keys]
forw-line = "down"
```

### Theme sources

A port's theme takes the first source the tool has ([vision.md](vision.md#themes)):

| Source     | Writes                                                          |
| ---------- | --------------------------------------------------------------- |
| `builtin`  | The tool's setting selecting its own theme                      |
| `upstream` | The official port's file for the flavour, copied or included    |
| `palette`  | `{role}` values filled from the palette, as above               |

```toml
[target.builtin.catppuccin-frappe]
"appearance.colorscheme" = "catppuccin-frappe"

[target.upstream]
catppuccin-frappe = "themes/frappe/{accent}.yml"
```

An upstream file replaces the template of a `block` or `file` target, or is merged
setting by setting into an `edit` target (YAML only). `{accent}` in its path takes
the accent's name. A theme with neither falls back to `theme` and the template, so
every port keeps a palette version. A setting the new source no longer writes, as
after a theme change, goes back to its backup value.

Upstream themes are committed under `ports/<tool>/themes/` by
`scripts/sync-themes.sh`, from pinned commits, with their licence and a `SOURCE`
file. A refresh is a reviewed diff and a port version bump, which re-applies them.
The build never fetches them.

A tool that imports a theme file gets a list entry: `"general.import[=~/.config/alacritty/vimify.toml]"`
adds that string to the list after the user's entries, and revert takes it back out.

### Sources and versioning

- `ports/` is embedded at build time (`include_dir`), so vimify works offline.
- Ports change only with a vimify release. Fetching them separately is not built.
- `schema` gates the port format. vimify rejects ports with an unknown schema.

A port covers a range of tool versions: `versions = ">=0.40, <0.60"`, default all.
Add a new port only when a tool's config changes, as `ports/<tool>/<dir>/port.toml`.
The scan runs `version_args` (default `--version`) and takes the first dotted number,
or with `integer_version = true` the first number (`less 710` is 710.0.0).

| Tool version                        | Port    | Shown       |
| ----------------------------------- | ------- | ----------- |
| In a range                          | That one | `supported` |
| Outside, same major as nearest port | Nearest | `supported` |
| Outside, another major              | Nearest | `untested`  |
| Unknown                             | Newest  | `untested`  |

Ports always lag releases, so being outside a range is not worth a warning by itself.
A tool crossing into another port plans as `update`, because the port version differs.

`refresh_args` run against the tool's binary after apply and revert, found in the
scan dirs, for a tool that reads config through a cache (`bat cache --build`). A failed
refresh warns: the files are already right.

`port.toml` is checked on load: `paths` stay in
config locations, and `version_args` and `refresh_args` are plain words. Neither replaces review: a
config can run commands, and that is only visible by reading the port.

### Key notation

Reference keys use vim notation (`j`, `G`, `<C-u>`). Each port picks a
built-in notation that converts it (`angle` → `<c-u>`, `plus` → `ctrl+u`, ...).
A key the notation cannot express skips that binding with a warning.

## Writing

### Strategies

| Strategy  | Writes                                       | Revert                         |
| --------- | -------------------------------------------- | ------------------------------ |
| `edit`    | Owned key paths in place, via format editor  | Restore owned keys             |
| `block`   | Region between `# >>> vimify >>>` markers    | Remove block                   |
| `file`    | Whole file from the template or upstream theme: a file vimify owns, or a tool's theme file it takes over | Restore backup or delete, edits included |
| `include` | Not implemented: only for a tool `edit` and `block` cannot reach ([stack.md](stack.md#tool-config-editing)) | |

Format editors share one trait so `yaml-edit` stays swappable
([stack.md](stack.md#tool-config-editing)):

```rust
trait Editor {
    fn set(&mut self, path: &[&str], value: Value) -> Result<()>;
    fn get(&self, path: &[&str]) -> Option<Value>;
    fn restore(&mut self, path: &[&str], original: &str) -> Result<()>;
    fn render(&self) -> String;
}
```

A block is an editor too, with its text as the one owned setting.

### Backups and state

- Before vimify modifies or deletes an existing file, it copies it to the first
  free name of `config.yml.bak`, `config.yml.bak.1`, `config.yml.bak.2`, ...
  An existing file is never overwritten, whoever created it.
- Only the user's content is backed up. A file whose hash matches what vimify
  last wrote gets no backup on update or revert. A `.bak*` with the same bytes
  and mode is reused instead of copied, so apply/revert cycles keep one backup.
- State keeps the backup from the first apply as the revert target, so revert
  restores the pre-vimify file however many re-applies happened since.
- Creating a missing file needs no backup. State records none, and `edit` revert
  deletes the file. A first apply over an existing file always backs up, even if
  the output is identical, or revert would delete the user's file.
- Order is backup, state, write. A crash before the write leaves state pointing
  at a real backup; a failed write rolls state back.
- Revert backs up a file edited since apply, and removes state last. A lost
  backup fails the revert and changes nothing.
- An untouched file (hash match) reverts to the first backup, byte for byte. An
  edited file reverts key by key: each owned key takes its value from the first
  backup, or goes away with any parent vimify created. Other edits survive.
- Writes are atomic (temp file + rename), through symlinks.
- State lives in `~/.local/state/vimify/state.toml`, per tool: port version, hash
  of the reference used, and per file its path, backup path, hash after apply,
  format, strategy, and each owned key with the value written.
- The reference hash covers `theme` and `keys` only. `tools` lives in the same
  file and changes on every selection, so it must not trigger a re-apply.
- An owned key differing from the value written = `modified`. Edits to anything
  else do not count, nor does a tool rewriting its own config. The tool plans as
  `update` or `revert`, and the confirm screen lists it as updated manually, to
  force. Editing a block counts; lines around it do not.

## Crate layout

Single crate, `lib.rs` + thin `main.rs`. Everything except `ui` is testable
without a terminal.

```
src/
  main.rs       subcommands, run ui
  reference.rs  load/save reference config
  actions.rs    action vocabulary, key notations
  palette.rs    roles, embedded palettes
  port.rs       port schema, embedded + user loader, validation
  scan.rs       scan dirs, detection, progress events
  plan.rs       desired state × state → ops
  render.rs     port + reference → values / template text
  edit/         Editor impls: toml, yaml, lines, block
  backup.rs     reuse an identical .bak, else copy to the first free name
  apply.rs      strategies, atomic write, revert
  state.rs      applied-state store
  ui/           ratatui screens: config, scan, select, rollback, confirm
```

## Testing

- Every port ships golden tests: `tests/*.input.*` rendered with a fixed
  reference must equal `*.expected.*`. CI runs them for all ports.
  `VIMIFY_BLESS=1 cargo test golden` rewrites the expected files, to review as a diff.
- Every port renders under every theme and accent, which covers the sources the
  golden files' default theme does not.
- Editors get preservation tests (comments, ordering, quoting, flow style).
- `apply`/`revert` run against a temp `$HOME`.
- `docker/e2e.sh` drives the real vimify and real tools through tmux. Every port
  needs a scenario there: golden tests prove the output, only the tool proves it
  works. Test keys the tool does not bind by default, and that its own keys survive.

## Open questions

- RON (gitui `key_bindings.ron`, `theme.ron`): no format-preserving editor.
