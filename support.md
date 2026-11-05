# Supported tools

Updated with every port change. In the app, `i` on a tool shows the same.

✅ mapped · ⚠️ partly, see limitations · ➖ fixed by the tool, not rebindable · ❌ not mapped

| Tool       | Versions | left | down | up | right | yank | delete | quit | Theme |
| ---------- | -------- | :--: | :--: | :-: | :---: | :--: | :----: | :--: | :---: |
| bat        | ≥ 0.25   | ❌   | ❌   | ❌ | ❌    | ❌   | ❌     | ❌   | ⚠️    |
| btop       | ≥ 1.3    | ➖   | ➖   | ➖ | ➖    | ❌   | ❌     | ➖   | ✅    |
| gitui      | —        | ❌   | ❌   | ❌ | ❌    | ❌   | ❌     | ❌   | ❌    |
| k9s        | ≥ 0.40   | ❌   | ➖   | ➖ | ❌    | ❌   | ❌     | ❌   | ✅    |
| lazydocker | ≥ 0.25   | ❌   | ❌   | ❌ | ❌    | ❌   | ❌     | ➖   | ✅    |
| lazygit    | ≥ 0.66   | ✅   | ✅   | ✅ | ✅    | ✅   | ✅     | ✅   | ✅    |
| lazysql    | ≥ 0.7    | ❌   | ✅   | ✅ | ❌    | ✅   | ✅     | ❌   | ✅    |
| lfk        | ≥ 0.19   | ➖   | ➖   | ➖ | ➖    | ❌   | ❌     | ❌   | ✅    |
| yazi       | ≥ 26.9   | ✅   | ✅   | ✅ | ✅    | ✅   | ✅     | ✅   | ✅    |

Outside its range a tool still gets the nearest port, shown as `untested` on another
major version ([architecture.md](architecture.md#sources-and-versioning)).

## Limitations

- **bat**: no keybindings; it pages through less. Theme is `ansi`, so syntax colours
  follow the terminal palette: bat ships no Catppuccin theme, and a custom one needs
  `bat cache --build`.
- **btop**: keys are btop's vim mode, hardcoded to h, j, k and l. Rebinding them in
  the reference has no effect. btop rewrites `btop.conf` on exit and drops comments.
- **gitui**: not supported. Its config is RON, which has no format-preserving editor.
- **k9s**: theme only, through a `vimify` skin. j and k are built in. A skin set per
  cluster or context wins over it.
- **lazydocker**: theme only; it has no configurable keybindings.
- **lazygit**: quit takes the one slot lazygit has, so ctrl+c stops quitting. Yank covers the file and commit copy keys only.
- **lazysql**: left and right not mapped: H and L switch panes, h and l move between
  cells.
- **lfk**: theme only; navigation is fixed, and resource deletion stays on D.
- **yazi**: keys cover the file list only; the task, preview and input layers keep
  their own. A keymap written as `[[mgr.prepend_keymap]]` tables is refused: use the
  inline array.

## Files written

| Tool       | Files                                                         |
| ---------- | ------------------------------------------------------------- |
| bat        | `bat/config` (marked block)                                   |
| btop       | `btop/btop.conf`, `btop/themes/vimify.theme`                  |
| k9s        | `k9s/config.yaml`, `k9s/skins/vimify.yaml`                    |
| lazydocker | `lazydocker/config.yml`                                       |
| lazygit    | `lazygit/config.yml`                                          |
| lazysql    | `lazysql/config.toml`                                         |
| lfk        | `lfk/config.yaml`                                             |
| yazi       | `yazi/keymap.toml`, `yazi/theme.toml`                         |

Paths are relative to `$XDG_CONFIG_HOME`, or a tool's own variable when set
(`$LG_CONFIG_FILE`, `$K9S_CONFIG_DIR`, `$LFK_CONFIG_DIR`, `$YAZI_CONFIG_HOME`, ...).
