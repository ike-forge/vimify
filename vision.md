# Vision

One set of vim-style keybindings and one theme across every CLI/TUI tool on the
machine, applied from a single reference config. Same goal as Omarchy, scoped to
the terminal and independent of the distro or window manager.

## Principles

- **Opt-in per tool.** Nothing is touched without the user selecting it.
- **Minimal footprint.** Only keys vimify owns (keybindings, theme, possibly
  layout) are written. Every other setting in a tool's config is preserved.
- **Always reversible.** Every file is backed up before it is modified.
- **Common ground, not full coverage.** The reference config covers shared
  motions and actions, not every action of every tool.

## User flow

1. Install vimify through a package manager (Homebrew, apt, ...).
2. First run scans common install locations. Only tools vimify knows are
   reported; unknown tools are ignored silently.
3. User chooses: **select all** (recommended) or **select manually**.
4. A warning states that config for selected tools will be overridden.
5. For each selected tool, the existing config is backed up next to the original
   (`~/.config/lazysql/config.yaml` → `~/.config/lazysql/config.yaml.bak`), then
   the vimify-owned keys are written.
6. The selection is saved. On later runs, previously excluded tools stay
   unchecked; newly installed known tools are offered.

## Operations

Per tool, or for all supported tools at once:

- **install** — apply the reference config.
- **revert / uninstall** — take vimify's keys back out, keeping any other edits,
  and drop the tool from the selection.

## Reference config

The single source of truth the user edits. Example:

```toml
theme = "catppuccin-frappe"

[keys]
left = "h"
down = "j"
up = "k"
right = "l"
yank = "y"
delete = "d"
```

The reference config, and optionally the generated tool configs, are meant to be
committed to the user's dotfiles. Committing tool configs only matters when the
user also customises them outside vimify.

## Tool support

Each supported tool has a mapping that translates the reference config into that
tool's config format and location.

- Ports live in this repository and are added by PR. vimify fetches them at
  startup, so a new port does not need a vimify release, and ships a copy for
  offline use.

## Ideas

Not committed to; brainstorm before building, drop if not useful.

- **Profiles.** Named sets of reference config. `vimify` uses the default
  profile; `vimify scan <profile>` and `vimify apply <profile>` target another.

## Design

See [stack.md](stack.md) and [architecture.md](architecture.md).
