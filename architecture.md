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
   (`applied`, `modified`, `update`). Initially checked: every supported tool not
   in `exclusions`. Unsupported tools are greyed out at the bottom: they cannot be
   checked and never enter `exclusions`, so they are offered once a port covers
   them. `i` opens a modal with what the port covers.
   A config pane left of the tools shows the theme and keys; `h`/`l` switch panes,
   `i` edits a value. Edits are saved on accept, writing only the values that
   changed, and plan with the edited reference's hash.
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
format = "yaml"            # toml | yaml | ron | lines
strategy = "edit"          # edit | block | include
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
Golden tests pick their target by file stem: `theme.input.toml` tests the target
writing `theme.toml`. Any other stem tests the first target.

TOML settings reach into arrays of inline tables by a field of the entry:
`"mgr.prepend_keymap[run=arrow next].on" = "down"`. A missing entry is appended, after
the user's, and revert takes it back out.

`lines` targets edit `key = value` lines, with values verbatim. A `block` target is
one template instead of settings:

```toml
[[target]]
format = "lines"
strategy = "block"
paths = ["$XDG_CONFIG_HOME/bat/config"]
template = '--theme="ansi"'
```

### Sources and versioning

- `ports/` is embedded at build time (`include_dir`), so vimify works offline.
- Ports change only with a vimify release. Fetching them separately is not built.
- `schema` gates the port format. vimify rejects ports with an unknown schema.

A port covers a range of tool versions: `versions = ">=0.40, <0.60"`, default all.
Add a new port only when a tool's config changes, as `ports/<tool>/<dir>/port.toml`.
The scan runs `version_args` (default `--version`) and takes the first dotted number.

| Tool version                        | Port    | Shown       |
| ----------------------------------- | ------- | ----------- |
| In a range                          | That one | `supported` |
| Outside, same major as nearest port | Nearest | `supported` |
| Outside, another major              | Nearest | `untested`  |
| Unknown                             | Newest  | `untested`  |

Ports always lag releases, so being outside a range is not worth a warning by itself.
A tool crossing into another port plans as `update`, because the port version differs.

`port.toml` is checked on load: `paths` stay in
config locations and `version_args` are plain words. Neither replaces review: a
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
| `include` | Not implemented: no port needs it yet        |                                |

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
- State lives in `~/.local/state/vimify/state.toml`, per tool: port version, hash
  of the reference used, and per file its path, backup path, hash after apply,
  format, strategy, and each owned key with the value written.
- The reference hash covers `theme` and `keys` only. `exclusions` live in the same
  file and change on every selection, so they must not trigger a re-apply.
- An owned key differing from the value written = `modified`. Edits to anything
  else do not count, nor does a tool rewriting its own config. The tool plans as
  `update` or `revert`, and the confirm screen lists it as updated manually, to
  force. Editing a block counts; lines around it do not.

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
  edit/         Editor impls: toml, yaml, lines, block
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
- `docker/e2e.sh` drives the real vimify and real tools through tmux. Every port
  needs a scenario there: golden tests prove the output, only the tool proves it
  works. Test keys the tool does not bind by default, and that its own keys survive.

## Open questions

- RON (gitui `key_bindings.ron`, `theme.ron`): no format-preserving editor.
