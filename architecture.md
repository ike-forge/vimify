# Architecture

## Concepts

| Term          | Meaning                                                              |
| ------------- | -------------------------------------------------------------------- |
| Reference     | User's `~/.config/vimify/config.toml`: theme, keys, scan paths, exclusions. Unset theme and actions take built-in defaults |
| Action        | A vimify-defined verb (`down`, `yank`, ...) bound to a key in the reference |
| Role          | A semantic colour slot (`accent`, `selection`, ...) filled by a palette |
| Palette       | Role → hex map for one theme (`catppuccin-frappe`). Ships with vimify |
| Port          | Data file describing how one tool maps actions and roles to its config |
| Plan          | List of file operations computed from reference + ports + state; shown before anything is written |
| State         | What vimify applied, per tool. Drives status and revert              |

## Flow

```
scan ──▶ select ──▶ plan ──▶ confirm ──▶ execute ──▶ result
```

1. **scan:** walk scan dirs, match binaries against port `binaries`. One progress
   tick per dir. Runs on a worker thread, events over a channel to the UI loop.
2. **select:** `apply to all`, `apply to none`, then detected tools with status
   (`applied`, `modified`, `update`). Initially checked: every detected tool not
   in `exclusions`.
3. **plan:** checkboxes are desired state, diffed against state:

   | Checked | Applied | Op                         |
   | ------- | ------- | -------------------------- |
   | yes     | no      | apply                      |
   | yes     | yes, older port version or changed reference | re-apply |
   | no      | yes     | revert                     |
   | no      | no      | none                       |

   Unchecked tools are written to `exclusions`.
4. **confirm:** the override warning, listing every file to apply and revert.
5. **execute:** per tool, independently. One failure does not stop the others.

## Ports

Mappings go from tool setting to vimify action/role, so a port only covers what
the tool supports. Ports cannot define new actions or roles. A binding takes a
slot the tool offers for alternates where it has one, so the tool's own keys
keep working.

A port with no editor or no mappings yet plans as `unsupported`.

```
ports/lazygit/
  port.toml
  tests/
    default.input.yml
    default.expected.yml
```

```toml
schema = 1
version = "0.1.0"
binaries = ["lazygit"]

[target]
format = "yaml"            # toml | yaml | json | lines
strategy = "edit"          # edit | block | include
key_notation = "angle"     # how the tool spells <C-u>
paths = [                  # first existing wins; first resolvable is created
  "$LG_CONFIG_FILE",
  "$XDG_CONFIG_HOME/lazygit/config.yml",
  "~/Library/Application Support/lazygit/config.yml",
]

# The -alt slots: the primary ones hold the arrow keys, which must keep working.
[keys]
"keybinding.universal.prevItem-alt" = "up"
"keybinding.universal.nextItem-alt" = "down"
"keybinding.universal.prevBlock-alt" = "left"
"keybinding.universal.nextBlock-alt" = "right"

[theme]
"gui.theme.activeBorderColor" = ["{accent}", "bold"]
"gui.theme.selectedLineBgColor" = ["{selection}"]
```

`lines` ports (`block`/`include` strategies) use text templates instead of key
paths:

```toml
[target]
format = "lines"
strategy = "include"
paths = ["$XDG_CONFIG_HOME/kitty/kitty.conf"]
include_file = "$XDG_CONFIG_HOME/kitty/vimify.conf"
include_line = "include vimify.conf"

template = """
map {keys.down} scroll_line_down
map {keys.up} scroll_line_up
selection_background {selection}
"""
```

### Sources and versioning

- v1: `ports/` is embedded at build time (`include_dir`). The DB version is the
  vimify version; each port also has its own `version`.
- Later: `~/.config/vimify/ports/<tool>/` overrides embedded ports, then
  fetching from community repos. Same format, so no loader changes.
- `schema` gates the port format. vimify rejects ports with an unknown schema.

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
| `include` | Own file + one include line in the tool file | Delete own file, remove line   |

Format editors share one trait so `yaml-edit` stays swappable
([stack.md](stack.md#tool-config-editing)):

```rust
trait Editor {
    fn set(&mut self, path: &[&str], value: Value) -> Result<()>;
    fn render(&self) -> String;
}
```

### Backups and state

- Before vimify modifies or deletes an existing file, it copies it to the first
  free name of `config.yml.bak`, `config.yml.bak.1`, `config.yml.bak.2`, ...
  An existing file is never overwritten, whoever created it.
- State keeps the backup from the first apply as the revert target, so revert
  restores the pre-vimify file however many re-applies happened since.
- Creating a missing file needs no backup. State records none, and `edit` revert
  deletes the file. A first apply over an existing file always backs up, even if
  the output is identical, or revert would delete the user's file.
- Order is backup, state, write. A crash before the write leaves state pointing
  at a real backup; a failed write rolls state back.
- Revert backs up the current file too, and removes state last. A lost backup
  fails the revert and changes nothing.
- An untouched file (hash match) reverts to the first backup, byte for byte. An
  edited file reverts key by key: each owned key takes its value from the first
  backup, or goes away with any parent vimify created. Other edits survive.
- Writes are atomic (temp file + rename), through symlinks.
- State lives in `~/.local/state/vimify/state.toml`, per tool: port version,
  target path, backup path, hash of the reference used, hash of the file after
  apply, format, and each owned key with the value written.
- The reference hash covers `theme` and `keys` only. `exclusions` live in the same
  file and change on every selection, so they must not trigger a re-apply.
- An owned key differing from the value written = `modified`. Edits to anything
  else do not count, nor does a tool rewriting its own config. The tool plans as
  `update` or `revert`, and the confirm screen lists it as updated manually, to
  force. `block`/`include` reverts are unaffected.

## Crate layout

Single crate, `lib.rs` + thin `main.rs`. Everything except `ui` is testable
without a terminal.

```
src/
  main.rs       terminal setup, run ui
  reference.rs  load/save reference config
  actions.rs    action vocabulary, key notations
  palette.rs    roles, embedded palettes
  port.rs       port schema, embedded + user loader, validation
  scan.rs       scan dirs, detection, progress events
  plan.rs       desired state × state → ops
  render.rs     port + reference → values / template text
  edit/         Editor impls: toml, yaml, json, lines
  backup.rs     copy to first free .bak name
  apply.rs      strategies, atomic write, revert
  state.rs      applied-state store
  ui/           ratatui screens: scan, select, confirm, result
```

## Testing

- Every port ships golden tests: `tests/*.input.*` rendered with a fixed
  reference must equal `*.expected.*`. CI runs them for all ports.
- Editors get preservation tests (comments, ordering, quoting, flow style).
- `apply`/`revert` run against a temp `$HOME`.

## Open questions

- Multi-file targets: yazi splits `keymap.toml` and `theme.toml`, and its
  keymaps are arrays of `{ on, run }`, not key paths.
- RON (gitui `key_bindings.ron`, `theme.ron`): no format-preserving editor.
