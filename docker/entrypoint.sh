#!/bin/sh
set -eu
# Passed as env rather than a bind mount: host paths may not be visible to the
# Docker daemon (Rancher Desktop on WSL).
if [ -n "${SANDBOX_KUBECONFIG:-}" ]; then
  mkdir -p "$HOME/.kube"
  printf '%s\n' "$SANDBOX_KUBECONFIG" > "$HOME/.kube/config"
  chmod 600 "$HOME/.kube/config"
fi
if [ -n "${SANDBOX_S3:-}" ]; then
  mkdir -p "$HOME/.aws"
  printf '[default]\naws_access_key_id = sandbox\naws_secret_access_key = sandbox-secret\n' \
    > "$HOME/.aws/credentials"
  printf '[default]\nregion = us-east-1\nendpoint_url = %s\n' "$SANDBOX_S3" > "$HOME/.aws/config"
fi
exec "$@"
