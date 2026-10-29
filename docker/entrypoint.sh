#!/bin/sh
set -eu
# Passed as env rather than a bind mount: host paths may not be visible to the
# Docker daemon (Rancher Desktop on WSL).
if [ -n "${SANDBOX_KUBECONFIG:-}" ]; then
  mkdir -p "$HOME/.kube"
  printf '%s\n' "$SANDBOX_KUBECONFIG" > "$HOME/.kube/config"
  chmod 600 "$HOME/.kube/config"
fi
exec "$@"
