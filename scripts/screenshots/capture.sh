#!/usr/bin/env bash
# Regenerates docs/img from the real tools. Everything runs under a throwaway
# HOME, so no config on this machine is read or written. Needs tmux, python3 with
# Pillow, and the tools installed.
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
out=$root/docs/img
work=$(mktemp -d)
trap 'tmux -L vimify-shots kill-server 2>/dev/null || true; rm -rf "$work"' EXIT
mkdir -p "$out" "$work/home"

cat > "$work/tmux.conf" <<'C'
set -g default-terminal "tmux-256color"
set -ga terminal-overrides ",*:RGB"
set -g status off
C
t() { tmux -L vimify-shots -f "$work/tmux.conf" "$@"; }
env_=(env HOME="$work/home" XDG_CONFIG_HOME="$work/home/.config" XDG_STATE_HOME="$work/home/.state"
  XDG_CACHE_HOME="$work/home/.cache" COLORTERM=truecolor TERM=tmux-256color)

# shoot NAME TITLE: capture the pane of session $NAME and render it.
shoot() {
  t capture-pane -e -p -N -t "$1" > "$work/$1.ansi"
  python3 "$root/scripts/screenshots/ansi2png.py" "$work/$1.ansi" "$out/$1.png" --title "$2"
}

cargo build --release --quiet --manifest-path "$root/Cargo.toml"
vimify=$root/target/release/vimify

t new -d -s select -x 96 -y 20 "${env_[@]}" "$vimify"
sleep 4
shoot select "vimify"
t send-keys -t select Enter
sleep 1
t rename-session -t select confirm
shoot confirm "vimify"
t send-keys -t confirm y
sleep 2
t kill-session -t confirm 2>/dev/null || true

# Same theme, different tools. k9s and lfk read the sandbox kind cluster, so this
# needs `mise run sandbox:cluster`. btop is left out: its pane lists host processes.
kind get kubeconfig --name vimify > "$work/kubeconfig"

repo=$work/demo
mkdir -p "$repo" && cd "$repo"
git init -q -b main . && git config user.email d@example.com && git config user.name demo
echo "# demo" > README.md && git add . && git commit -qm "Initial commit"
echo "fn main() {}" > main.rs && git add . && git commit -qm "Add entry point"
git checkout -qb feature/theme && echo "theme = 1" > theme.toml && git add . && git commit -qm "Add theme"
git checkout -q main && echo "wip" >> README.md && echo "new" > notes.txt
cd "$work"

size=(-x 110 -y 30)
t new -d -s lazygit "${size[@]}" -c "$repo" "${env_[@]}" lazygit
t new -d -s lazydocker "${size[@]}" "${env_[@]}" lazydocker
t new -d -s k9s "${size[@]}" "${env_[@]}" KUBECONFIG="$work/kubeconfig" k9s
t new -d -s lfk "${size[@]}" "${env_[@]}" KUBECONFIG="$work/kubeconfig" lfk
sleep 5
t send-keys -t lazygit Enter
t send-keys -t k9s 0
sleep 2
for s in lazygit lazydocker k9s lfk; do shoot "$s" "$s"; done

python3 "$root/scripts/screenshots/collage.py" "$out" "$out/themed.png"
