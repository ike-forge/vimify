#!/usr/bin/env bash
# Copies the official Catppuccin ports into ports/<tool>/themes/ at the pinned commits.
# Bump a commit, run, review the diff, and bump the port's version so it re-applies.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# tool repo commit file...: paths in the upstream repository, globs allowed.
pins=(
  "alacritty catppuccin/alacritty 3471f96779050275eacc173b540e2d47e7b8820f catppuccin-*.toml"
  "bat catppuccin/bat 6810349b28055dce54076712fc05fc68da4b8ec0 themes/*.tmTheme"
  "btop catppuccin/btop f437574b600f1c6d932627050b15ff5153b58fa3 themes/*.theme"
  "gitui catppuccin/gitui df2f59f847e047ff119a105afff49238311b2d36 themes/*.ron"
  "k9s catppuccin/k9s fdbec82284744a1fc2eb3e2d24cb92ef87ffb8b4 dist/catppuccin-frappe.yaml dist/catppuccin-latte.yaml dist/catppuccin-macchiato.yaml dist/catppuccin-mocha.yaml"
  "kitty catppuccin/kitty 177d447f0c3626959ef001b4605f5d844541fd63 themes/frappe.conf themes/latte.conf themes/macchiato.conf themes/mocha.conf"
  "lazygit catppuccin/lazygit 1a4b26cbc2b5be05cc5494d0d7f764120a2cf86d themes-mergable"
  "yazi catppuccin/yazi d62802be39210ea10e54b3e3b09735c6cb9e57c1 themes"
)

for pin in "${pins[@]}"; do
  read -r tool repo commit files <<<"$pin"
  src="$work/$tool"
  git init -q "$src"
  git -C "$src" fetch -q --depth 1 "https://github.com/$repo.git" "$commit"
  git -C "$src" checkout -q FETCH_HEAD
  dest="$root/ports/$tool/themes"
  rm -rf "$dest"
  mkdir -p "$dest"
  # A directory is copied with its subdirectories: lazygit and yazi nest by flavour.
  (cd "$src" && for f in $files; do
    if [ -d "$f" ]; then cp -R "$f"/. "$dest"/; else cp "$f" "$dest"/; fi
  done)
  cp "$src/LICENSE" "$dest/LICENSE"
  printf 'https://github.com/%s/tree/%s\n' "$repo" "$commit" >"$dest/SOURCE"
done
