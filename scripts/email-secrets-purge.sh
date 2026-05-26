#!/usr/bin/env bash
# Delete all HTTP_NU_EMAIL_* items from the macOS keychain via fnox. Mirror
# of `email:secrets:generate`. Idempotent -- per-item delete is best-effort.
# Used by `mise run email:secrets:purge`.
#
# This wipes:
#   - HTTP_NU_EMAIL_AUTH_TOKEN         (plugin <-> Worker bearer)
#   - HTTP_NU_EMAIL_WEBHOOK_HMAC_KEY   (Worker -> http-nu signing)
#   - HTTP_NU_EMAIL_ADMIN_TOKEN        (demo /send bearer)
#   - HTTP_NU_EMAIL_WORKER_URL         (deployed Worker URL)
#   - HTTP_NU_EMAIL_WEBHOOK_URL        (http-nu URL the Worker POSTs to)
#   - HTTP_NU_EMAIL_SENDER_DOMAIN      (sender domain)
#   - HTTP_NU_EMAIL_CLOUDFLARE_API_TOKEN (wrangler auth)
#
# Pass --yes (or run with EMAIL_PURGE_CONFIRM=1) to skip the interactive
# confirmation. Default is to prompt.

set -euo pipefail

confirm="${EMAIL_PURGE_CONFIRM:-}"
if [ "${1:-}" = "--yes" ]; then
  confirm=1
fi

items=(
  HTTP_NU_EMAIL_AUTH_TOKEN
  HTTP_NU_EMAIL_WEBHOOK_HMAC_KEY
  HTTP_NU_EMAIL_ADMIN_TOKEN
  HTTP_NU_EMAIL_WORKER_URL
  HTTP_NU_EMAIL_WEBHOOK_URL
  HTTP_NU_EMAIL_SENDER_DOMAIN
  HTTP_NU_EMAIL_CLOUDFLARE_API_TOKEN
)

if [ -z "$confirm" ]; then
  echo "About to delete the following keychain items:"
  for item in "${items[@]}"; do
    echo "  - $item"
  done
  echo
  echo -n "Proceed? [y/N] "
  read -r ans
  case "$ans" in
    y|Y|yes|YES) ;;
    *) echo "Aborted."; exit 1 ;;
  esac
fi

for item in "${items[@]}"; do
  # `fnox delete -p keychain` removes a single item. Non-zero on missing
  # items, which we treat as a no-op (purge is idempotent).
  if fnox delete -p keychain "$item" 2>/dev/null; then
    echo "deleted: $item"
  else
    echo "(absent): $item"
  fi
done

echo
echo "Done. fnox.toml itself is unchanged -- the env-var-to-keychain mapping"
echo "stays in place, so a future 'email:secrets:generate' will repopulate."
