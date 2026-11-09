# Supported tools

Updated with every port change. In the app, `i` on a tool shows the same.

Ordered from most to least covered.

✅ mapped · 🟡 partly, see limitations · ➖ fixed by the tool, not rebindable · ❌ not mapped ·
blank: the tool has no such action

| Tool       | Versions | left | down |  up  | right | yank | delete | quit | Theme |
| ---------- | -------- | :--: | :--: | :--: | :---: | :--: | :----: | :--: | :---: |
| lazygit    | ≥ 0.66   |  ✅  |  ✅  |  ✅  |  ✅   |  ✅  |   ✅   |  ✅  |  ✅   |
| yazi       | ≥ 26.9   |  ✅  |  ✅  |  ✅  |  ✅   |  ✅  |   ✅   |  ✅  |  ✅   |
| lazysql    | ≥ 0.7    |  ❌  |  ✅  |  ✅  |  ❌   |  ✅  |   ✅   |  ❌  |  ✅   |
| less       | ≥ 582    |  ✅  |  ✅  |  ✅  |  ✅   |      |        |  ✅  |  ❌   |
| stu        | ≥ 0.7    |  🟡  |  ✅  |  ✅  |  🟡   |  ✅  |        |  ❌  |  ➖   |
| lazydocker | ≥ 0.25   |  ➖  |  ➖  |  ➖  |  ➖   |      |   ➖   |  ➖  |  ✅   |
| btop       | ≥ 1.3    |  ➖  |  ➖  |  ➖  |  ➖   |      |        |  ➖  |  ✅   |
| bat        | ≥ 0.25   |  ➖  |  ➖  |  ➖  |  ➖   |      |        |  ➖  |  🟡   |
| lfk        | ≥ 0.19   |  ➖  |  ➖  |  ➖  |  ➖   |  ❌  |   ❌   |  ❌  |  ✅   |
| k9s        | ≥ 0.40   |  ❌  |  ➖  |  ➖  |  ❌   |  ❌  |   ❌   |  ❌  |  ✅   |
| gitui      | —        |  ❌  |  ❌  |  ❌  |  ❌   |  ❌  |   ❌   |  ❌  |  ❌   |

Outside its range a tool still gets the nearest port, shown as `untested` on another
major version ([architecture.md](architecture.md#sources-and-versioning)).

## Limitations

- **lazygit**: quit takes the one slot lazygit has, so ctrl+c stops quitting. Yank covers the file and commit copy keys only.
- **yazi**: keys cover the file list only; the task, preview and input layers keep
  their own. A keymap written as `[[mgr.prepend_keymap]]` tables is refused: use the
  inline array.
- **lazysql**: left and right not mapped: H and L switch panes, h and l move between
  cells.
- **stu**: left and right scroll and switch tabs in the detail, preview and dialog
  views; the bucket and object lists keep enter and backspace. A binding replaces
  stu's key, so copy details moves off r. stu cannot delete. quit is not mapped:
  stu quits on that key even while typing a filter. Colours are built in.
- **btop**: keys are btop's vim mode, hardcoded to h, j, k and l. Rebinding them in
  the reference has no effect. btop rewrites `btop.conf` on exit and drops comments.
- **lfk**: theme only; navigation is fixed, and resource deletion stays on D.
- **k9s**: theme only, through a `vimify` skin. j and k are built in. A skin set per
  cluster or context wins over it.
- **lazydocker**: theme only; it has no configurable keybindings. Its own h, j, k, l, d
  and q are already vim's.
- **bat**: no keybindings; output longer than a screen pages through less, so its keys
  are less's: j, k and q, and sideways scrolling. The less port rebinds them. Theme is
  `ansi`, so syntax colours follow the terminal palette: bat ships no Catppuccin
  theme, and a custom one needs `bat cache --build`.
- **less**: keys only, and for every program paging through less: bat, man, git.
  left and right scroll sideways; h no longer opens help, H still does. BusyBox
  less reads no lesskey file.
- **gitui**: not supported. Its config is RON, which has no format-preserving editor.

## Files written

| Tool       | Files                                                         |
| ---------- | ------------------------------------------------------------- |
| lazygit    | `lazygit/config.yml`                                          |
| yazi       | `yazi/keymap.toml`, `yazi/theme.toml`                         |
| lazysql    | `lazysql/config.toml`                                         |
| stu        | `~/.stu/keybindings.toml`                                     |
| btop       | `btop/btop.conf`, `btop/themes/vimify.theme`                  |
| lfk        | `lfk/config.yaml`                                             |
| k9s        | `k9s/config.yaml`, `k9s/skins/vimify.yaml`                    |
| less       | `lesskey` (marked block), or `~/.lesskey` if it exists        |
| lazydocker | `lazydocker/config.yml`                                       |
| bat        | `bat/config` (marked block)                                   |

Paths are relative to `$XDG_CONFIG_HOME` unless they start with `~`, or a tool's own
variable when set (`$LG_CONFIG_FILE`, `$K9S_CONFIG_DIR`, `$LFK_CONFIG_DIR`,
`$STU_ROOT_DIR`, `$YAZI_CONFIG_HOME`, `$LESSKEYIN`, ...).
