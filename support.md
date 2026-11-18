# Supported tools

Updated with every port change. In the app, `i` on a tool shows the same.

Ordered from most to least covered.

✅ mapped · 🟡 partly, see limitations · ➖ fixed by the tool, not rebindable · ❌ not mapped ·
blank: the tool has no such action

| Tool       | Versions | left | down |  up  | right | yank | delete | quit | Theme |
| ---------- | -------- | :--: | :--: | :--: | :---: | :--: | :----: | :--: | :---: |
| lazygit    | ≥ 0.66   |  ✅  |  ✅  |  ✅  |  ✅   |  ✅  |   ✅   |  ✅  |  ✅   |
| yazi       | ≥ 26.9   |  ✅  |  ✅  |  ✅  |  ✅   |  ✅  |   ✅   |  ✅  |  ✅   |
| gitui      | ≥ 0.28   |  ✅  |  ✅  |  ✅  |  ✅   |  ✅  |   ✅   |  ✅  |  ✅   |
| tmux       | ≥ 2.4    |  ✅  |  ✅  |  ✅  |  ✅   |  ✅  |        |  ✅  |  ✅   |
| lfk        | ≥ 0.19   |  ✅  |  ✅  |  ✅  |  ✅   |  ✅  |   ✅   |  ➖  |  ✅   |
| lazysql    | ≥ 0.7    |  ❌  |  ✅  |  ✅  |  ❌   |  ✅  |   ✅   |  ✅  |  ✅   |
| less       | ≥ 582    |  ✅  |  ✅  |  ✅  |  ✅   |      |        |  ✅  |  ❌   |
| stu        | ≥ 0.7    |  🟡  |  ✅  |  ✅  |  🟡   |  ✅  |        |  ❌  |  ➖   |
| lazydocker | ≥ 0.25   |  ➖  |  ➖  |  ➖  |  ➖   |      |   ➖   |  ➖  |  ✅   |
| btop       | ≥ 1.3    |  ➖  |  ➖  |  ➖  |  ➖   |      |        |  ➖  |  ✅   |
| alacritty  | ≥ 0.13   |  ❌  |  ❌  |  ❌  |  ❌   |  ❌  |        |      |  ✅   |
| kitty      | ≥ 0.41   |  ➖  |  ➖  |  ➖  |  ➖   |  ➖  |        |  ➖  |  ✅   |
| bat        | ≥ 0.25   |  ➖  |  ➖  |  ➖  |  ➖   |      |        |  ➖  |  🟡   |
| k9s        | ≥ 0.40   |  ➖  |  ➖  |  ➖  |  ➖   |  ➖  |   ➖   |  ➖  |  ✅   |

| Tool       | top | bottom | page_up | page_down | search | help |
| ---------- | :-: | :----: | :-----: | :-------: | :----: | :--: |
| lazygit    | ❌  |   ❌   |   🟡    |    🟡     |   ✅   |  🟡  |
| yazi       | ❌  |   ✅   |   ✅    |    ✅     |   ✅   |  ❌  |
| gitui      | ✅  |   ✅   |   ✅    |    ✅     |   ✅   |  ✅  |
| tmux       | ✅  |   ✅   |   ✅    |    ✅     |   ✅   |  ❌  |
| lfk        | ➖  |   ➖   |   ➖    |    ➖     |   ✅   |  🟡  |
| lazysql    | ✅  |   ✅   |   🟡    |    🟡     |   ✅   |  ✅  |
| less       | ✅  |   ✅   |   ✅    |    ✅     |   ✅   |  ❌  |
| stu        | ✅  |   ✅   |   ✅    |    ✅     |   ✅   |  ✅  |
| lazydocker | ➖  |   ➖   |   ➖    |    ➖     |   ➖   |  ➖  |
| btop       | ➖  |   ➖   |   ➖    |    ➖     |   ➖   |  ➖  |
| alacritty  | ❌  |   ❌   |   ❌    |    ❌     |   ❌   |      |
| kitty      | ➖  |   ➖   |   ➖    |    ➖     |   ➖   |  ➖  |
| bat        | ➖  |   ➖   |   ➖    |    ➖     |   ➖   |  ➖  |
| k9s        | ➖  |   ➖   |   ➖    |    ➖     |   ➖   |  ➖  |

Outside its range a tool still gets the nearest port, shown as `untested` on another
major version ([architecture.md](architecture.md#sources-and-versioning)).

## Limitations

- **lazygit**: quit takes the one slot lazygit has, so ctrl+c stops quitting. Yank
  covers the file and commit copy keys only. top and bottom stay on `<` and `>`: g and
  G already reset, pop stash and open pull requests. page_up and page_down scroll the
  main view. help opens the keybindings menu.
- **yazi**: top stays on gg: g is yazi's goto prefix. help is not mapped: ? is find
  previous, as in vim. Keys cover the file list only; the task, preview and input
  layers keep their own. A keymap written as `[[mgr.prepend_keymap]]` tables is
  refused: use the inline array.
- **gitui**: one key per move, so the arrow keys, Home, End, PageUp and PageDown stop
  moving. search takes the file, branch and log find keys. Help moves from h to ?.
  delete takes discard, reset lines, drop stash and delete branch or tag, which all
  confirm first.
- **lazysql**: left and right not mapped: H and L switch panes, h and l move between
  cells. page_up and page_down page the tree; the table keeps < and > for result
  pages.
- **stu**: left and right scroll and switch tabs in the detail, preview and dialog
  views; the bucket and object lists keep enter and backspace. A binding replaces
  stu's key, so copy details moves off r and pages off ctrl+f and ctrl+b. stu cannot
  delete. quit is not mapped: stu quits on that key even while typing a filter.
  Colours are built in.
- **btop**: keys are btop's vim mode, hardcoded to h, j, k and l. Rebinding them in
  the reference has no effect. btop rewrites `btop.conf` on exit and drops comments.
- **tmux**: keys cover copy mode only; prefix bindings stay as they are. tmux's own
  copy-mode-vi keys keep working beside these. help is not mapped: ? is backward
  search, as in vim. Takes effect in a new tmux server, or after `tmux source-file`.
- **lfk**: quit, top, bottom and paging stay on q, g, G, ctrl+u and ctrl+d, which lfk
  hardcodes. help takes the keymaps panel (`?`); F1 still opens the help screen.
  delete takes resource deletion, which confirms first, and diff moves from d to D.
- **k9s**: theme only, through a `vimify` skin. k9s has no keymap; j and k are built
  in. A skin set per cluster or context wins over it.
- **lazydocker**: theme only; it has no configurable keybindings. Its own h, j, k, l,
  d and q are already vim's.
- **alacritty**: theme only. A vi mode binding needs its key, modifiers and mode
  written together, which vimify cannot do yet. Vi mode's own keys already are h, j,
  k, l, y, g, G, ctrl+u, ctrl+d and /. The theme overrides any imported one.
- **kitty**: theme only, including the 16 terminal colours that bat and other `ansi`
  themes draw with. kitty has no modal keys, so binding a plain j would stop it being
  typed. Scrollback opens in less, which the less port rebinds. A theme chosen later
  with `kitty +kitten themes` lands after the vimify block and wins.
- **bat**: no keybindings; output longer than a screen pages through less, so its keys
  are less's: j, k and q, and sideways scrolling. The less port rebinds them. Theme is
  `ansi`, so syntax colours follow the terminal palette: bat ships no Catppuccin
  theme, and a custom one needs `bat cache --build`.
- **less**: keys only, and for every program paging through less: bat, man, git. left
  and right scroll sideways; h no longer opens help, H still does. help is not mapped:
  ? is backward search, as in vim. BusyBox less reads no lesskey file.

## Files written

| Tool       | Files                                                         |
| ---------- | ------------------------------------------------------------- |
| lazygit    | `lazygit/config.yml`                                          |
| yazi       | `yazi/keymap.toml`, `yazi/theme.toml`                         |
| gitui      | `gitui/key_bindings.ron`, `gitui/theme.ron`                   |
| lazysql    | `lazysql/config.toml`                                         |
| stu        | `~/.stu/keybindings.toml`                                     |
| btop       | `btop/btop.conf`, `btop/themes/vimify.theme`                  |
| lfk        | `lfk/config.yaml`                                             |
| k9s        | `k9s/config.yaml`, `k9s/skins/vimify.yaml`                    |
| less       | `lesskey` (marked block), or `~/.lesskey` if it exists        |
| lazydocker | `lazydocker/config.yml`                                       |
| bat        | `bat/config` (marked block)                                   |
| tmux       | `~/.tmux.conf` (marked block), or `tmux/tmux.conf` if only it exists |
| kitty      | `kitty/kitty.conf` (marked block)                             |
| alacritty  | `alacritty/alacritty.toml`, or `alacritty.toml`, `~/.alacritty.toml` if it exists |

Paths are relative to `$XDG_CONFIG_HOME` unless they start with `~`, or a tool's own
variable when set (`$LG_CONFIG_FILE`, `$K9S_CONFIG_DIR`, `$LFK_CONFIG_DIR`,
`$STU_ROOT_DIR`, `$YAZI_CONFIG_HOME`, `$LESSKEYIN`, `$KITTY_CONFIG_DIRECTORY`, ...).
