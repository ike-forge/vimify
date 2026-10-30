#!/usr/bin/env bash
# The real vimify and real tools, driven through tmux inside the sandbox image.
# Run with `mise run e2e`. Needs a clean ~ for each test, so never run it on a host.
set -u

export LANG=C.UTF-8

CFG=~/.config/lazygit/config.yml
STATE=~/.local/state/vimify/state.toml
REFERENCE=~/.config/vimify/config.toml
# #8caaee, #414559 and #51576d as tmux prints them.
ACCENT='38;2;140;170;238'
SELECTION='48;2;65;69;89'
BORDER='38;2;81;87;109'

ORIGINAL='# My lazygit setup
gui:
  # Needs a patched terminal font
  nerdFontsVersion: "3"
  theme:
    activeBorderColor:
      - red # mine
keybinding:
  universal:
    nextItem-alt: n # my own binding
git:
  autoFetch: false # corporate proxy
'

failures=0
current=

fail() {
  failures=$((failures + 1))
  echo "  FAIL $current: $1"
}

screen() { tmux -u capture-pane -t app -p "$@"; }

wait_for() {
  local tries=75
  until screen | grep -q -- "$1"; do
    tries=$((tries - 1))
    if [ "$tries" = 0 ]; then
      fail "timed out waiting for '$1'"
      screen | sed 's/^/    | /'
      return 1
    fi
    sleep 0.2
  done
}

close() { tmux -u kill-server 2>/dev/null; }

reset() {
  close
  rm -rf ~/.config/lazygit ~/.config/lazydocker ~/.config/lfk \
    ~/.config/vimify ~/.local/state/vimify
}

seed() {
  reset
  mkdir -p ~/.config/lazygit
  printf '%s' "$ORIGINAL" >"$CFG"
  cp "$CFG" /tmp/original
}

# Reaches the confirm screen. Arguments are keys for the select screen.
confirm() {
  close
  tmux -u new-session -d -s app -x 150 -y 40 'vimify 2>&1; echo EXIT=$?; sleep 60'
  wait_for 'apply to all' || return 1
  [ "$#" = 0 ] || tmux -u send-keys -t app "$@"
  tmux -u send-keys -t app Enter
  wait_for ' confirm'
}

# Accepts the confirm screen and waits for vimify to exit.
accept() {
  tmux -u send-keys -t app y
  wait_for 'EXIT=' && close_after_read
}

close_after_read() { OUTPUT=$(screen); close; }

apply() { confirm && accept; }
revert() { confirm j Space && accept; }

on_screen() { screen | grep -q -- "$1" || fail "screen lacks '$1'"; }
off_screen() { ! screen | grep -q -- "$1" || fail "screen shows '$1'"; }
in_output() { printf '%s' "$OUTPUT" | grep -q -- "$1" || fail "output lacks '$1'"; }

in_file() { grep -q -- "$2" "$1" || fail "$1 lacks '$2'"; }
not_in_file() { ! grep -q -- "$2" "$1" || fail "$1 still has '$2'"; }

is_original() { cmp -s "$1" /tmp/original || fail "$1 differs from the original"; }

is_text() {
  local want
  want=$(printf '%s' "$1")
  [ "$(cat "$CFG")" = "$want" ] || {
    fail "unexpected config"
    diff <(printf '%s\n' "$want") "$CFG" | sed 's/^/    /'
  }
}

exists() { [ -e "$1" ] || fail "$1 is missing"; }
absent() { [ ! -e "$1" ] || fail "$1 exists"; }

test_apply_patches_only_owned_keys() {
  seed
  confirm
  on_screen 'apply .*lazygit'
  accept
  in_output 'EXIT=0'
  is_text "# My lazygit setup
gui:
  # Needs a patched terminal font
  nerdFontsVersion: \"3\"
  theme:
    activeBorderColor:
      - '#8caaee'
      - bold
    defaultFgColor:
      - '#c6d0f5'
    inactiveBorderColor:
      - '#51576d'
    optionsTextColor:
      - '#8caaee'
    searchingActiveBorderColor:
      - '#e5c890'
      - bold
    selectedLineBgColor:
      - '#414559'
    unstagedChangesColor:
      - '#e78284'
keybinding:
  universal:
    nextItem-alt: j # my own binding
    nextBlock-alt: l
    prevBlock-alt: h
    prevItem-alt: k
git:
  autoFetch: false # corporate proxy"
  is_original "$CFG.bak"
  in_file "$STATE" "backup = \"$CFG.bak\""
}

test_rerun_is_current() {
  seed
  apply
  confirm
  on_screen 'Nothing to apply'
  on_screen 'current .*lazygit'
  absent "$CFG.bak.1"
}

test_rebinding_updates_and_keeps_the_first_backup() {
  seed
  apply
  mkdir -p ~/.config/vimify
  printf '[keys]\ndown = "m"\n' >>"$REFERENCE"
  confirm
  on_screen 'update .*lazygit'
  off_screen 'updated manually'
  accept
  in_file "$CFG" 'nextItem-alt: m'
  exists "$CFG.bak.1"
  is_original "$CFG.bak"
  in_file "$STATE" "backup = \"$CFG.bak\""
}

test_missing_config_is_created() {
  reset
  confirm
  on_screen 'create .*lazygit'
  accept
  in_file "$CFG" 'nextItem-alt: j'
  absent "$CFG.bak"
  not_in_file "$STATE" '^backup'
}

test_unusable_config_fails_without_touching_it() {
  seed
  printf -- '- not\n- a mapping\n' >"$CFG"
  cp "$CFG" /tmp/original
  apply
  in_output 'EXIT=1'
  in_output 'top level is not a mapping'
  is_original "$CFG"
  absent "$CFG.bak"
}

test_revert_of_an_untouched_file_is_byte_identical() {
  seed
  apply
  confirm j Space
  on_screen 'revert .*lazygit'
  off_screen 'updated manually'
  accept
  is_original "$CFG"
  not_in_file "$STATE" 'lazygit'
  confirm
  on_screen 'Nothing to apply'
}

test_unrelated_edits_are_not_manual_and_survive_revert() {
  seed
  apply
  sed -i 's/autoFetch: false # corporate proxy/autoFetch: true # by hand/' "$CFG"
  echo 'os: {editPreset: nvim} # by hand' >>"$CFG"
  confirm
  on_screen 'current .*lazygit'
  off_screen 'by hand'
  confirm j Space
  off_screen 'updated manually'
  accept
  is_text "${ORIGINAL/autoFetch: false # corporate proxy/autoFetch: true # by hand}os: {editPreset: nvim} # by hand"
}

test_manual_theme_edit_warns_on_apply() {
  seed
  apply
  sed -i "s/'#8caaee'/'#ff0000'/" "$CFG"
  confirm
  on_screen 'changed by hand'
  on_screen 'updated manually, force update'
  # Back to select, then quit: nothing is forced.
  tmux -u send-keys -t app n q
  wait_for 'EXIT='
  close_after_read
  in_file "$CFG" 'ff0000'
  confirm
  accept
  in_file "$CFG" '8caaee'
  not_in_file "$CFG" 'ff0000'
  confirm
  on_screen 'current .*lazygit'
}

test_manual_key_edit_warns_on_revert() {
  seed
  apply
  sed -i 's/prevItem-alt: k/prevItem-alt: i/' "$CFG"
  sed -i 's/autoFetch: false # corporate proxy/autoFetch: true # by hand/' "$CFG"
  confirm j Space
  on_screen 'changed by hand'
  on_screen 'updated manually, force revert'
  accept
  is_text "${ORIGINAL/autoFetch: false # corporate proxy/autoFetch: true # by hand}"
  in_file "$CFG.bak.1" 'prevItem-alt: i'
}

test_revert_of_a_created_file_removes_it() {
  reset
  apply
  revert
  absent "$CFG"
  in_file "$CFG.bak" 'nextItem-alt: j'
}

test_lost_backup_refuses_to_revert() {
  seed
  apply
  rm "$CFG.bak"
  cp "$CFG" /tmp/original
  revert
  in_output 'EXIT=1'
  is_original "$CFG"
  in_file "$STATE" 'lazygit'
}

lazygit_row() {
  screen -e | grep -an "$SELECTION" | cut -d: -f1 | head -1
}

press() {
  tmux -u send-keys -t app "$1"
  sleep 0.5
  [ "$(lazygit_row)" = "$2" ] || fail "$1 left the selection on row $(lazygit_row), want $2"
}

# Keys lazygit does not bind by default, so a pass proves the config was read. The
# arrows prove vimify left the tool's own keys alone.
test_lazygit_honours_what_vimify_wrote() {
  reset
  mkdir -p ~/.config/vimify
  printf '[keys]\ndown = "m"\n' >"$REFERENCE"
  apply
  close
  tmux -u new-session -d -s app -x 150 -y 40 -c ~/git-repo 'lazygit; sleep 60'
  wait_for 'Local branches\|Commits' || return
  sleep 1
  # First run only.
  if screen | grep -q 'Press <enter>'; then
    tmux -u send-keys -t app Enter
    sleep 1
  fi
  screen -e | grep -aq "$ACCENT" || fail 'accent colour not on screen'
  local start
  start=$(lazygit_row)
  [ -n "$start" ] || fail 'selection colour not on screen'
  local next=$((start + 1))
  press m "$next"
  press k "$start"
  press Down "$next"
  press Up "$start"
  press j "$start"
}

# Both drop to 256 colours on tmux's own TERM, where #8caaee and lfk's default round
# to the same colour.
TRUECOLOR='TERM=xterm-256color'

# With no daemon in the image, lazydocker draws its panels but reopens an error popup
# on every retry, hiding the selection. Dismiss it until the selection shows.
test_lazydocker_shows_the_palette() {
  reset
  apply
  close
  tmux -u new-session -d -s app -x 150 -y 40 "$TRUECOLOR lazydocker; sleep 60"
  local tries=50
  until screen -e | grep -aq "$SELECTION"; do
    tries=$((tries - 1))
    if [ "$tries" = 0 ]; then
      fail 'selection colour not on screen'
      return
    fi
    if screen | grep -q 'y/enter: yes'; then
      tmux -u send-keys -t app y
    fi
    sleep 0.2
  done
  screen -e | grep -aq "$ACCENT" || fail 'accent colour not on screen'
  screen -e | grep -aq "$BORDER" || fail 'border colour not on screen'
}

test_lfk_shows_the_palette() {
  reset
  apply
  close
  tmux -u new-session -d -s app -x 150 -y 40 "$TRUECOLOR lfk --demo; sleep 60"
  wait_for 'DEMO' || return
  sleep 2
  screen -e | grep -aq "$ACCENT" || fail 'accent colour not on screen'
  screen -e | grep -aq '48;2;140;170;238' || fail 'selected row not in the accent colour'
  # Its default, tokyonight.
  ! screen -e | grep -aq '122;162;247' || fail 'default theme still on screen'
}

tests=$(declare -F | awk '{print $3}' | grep '^test_')
for test in $tests; do
  current=$test
  before=$failures
  "$test"
  [ "$failures" = "$before" ] && echo "ok $test"
done
close

echo "$(echo "$tests" | wc -l) tests, $failures failures"
[ "$failures" = 0 ]
