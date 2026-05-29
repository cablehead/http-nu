#!/usr/bin/env nu
# Delete all HTTP_NU_EMAIL_* items from the macOS keychain via fnox. Mirror
# of `email:secrets:generate`. Idempotent -- per-item delete is best-effort.
# Used by `mise run email:secrets:purge`.
#
# This wipes:
#   - HTTP_NU_EMAIL_AUTH_TOKEN              (plugin <-> Worker bearer)
#   - HTTP_NU_EMAIL_WEBHOOK_HMAC_KEY        (Worker -> http-nu signing)
#   - HTTP_NU_EMAIL_ADMIN_TOKEN             (demo /send bearer)
#   - HTTP_NU_EMAIL_WORKER_URL              (deployed Worker URL)
#   - HTTP_NU_EMAIL_WEBHOOK_URL             (http-nu URL the Worker POSTs to)
#   - HTTP_NU_EMAIL_SENDER_DOMAIN           (sender domain)
#   - HTTP_NU_EMAIL_CLOUDFLARE_API_TOKEN    (wrangler auth)
#   - HTTP_NU_EMAIL_TEST_RECIPIENT_EMAIL    (smoke test inbox)
#
# Pass --yes (or set EMAIL_PURGE_CONFIRM=1) to skip the interactive
# confirmation. Default is to prompt.

const ITEMS = [
    HTTP_NU_EMAIL_AUTH_TOKEN
    HTTP_NU_EMAIL_WEBHOOK_HMAC_KEY
    HTTP_NU_EMAIL_ADMIN_TOKEN
    HTTP_NU_EMAIL_WORKER_URL
    HTTP_NU_EMAIL_WEBHOOK_URL
    HTTP_NU_EMAIL_SENDER_DOMAIN
    HTTP_NU_EMAIL_CLOUDFLARE_API_TOKEN
    HTTP_NU_EMAIL_TEST_RECIPIENT_EMAIL
]

def main [--yes] {
    let confirm = ($yes or (($env.EMAIL_PURGE_CONFIRM? | default "") | is-not-empty))

    if not $confirm {
        print "About to delete the following keychain items:"
        for item in $ITEMS { print $"  - ($item)" }
        print ""
        let ans = (input "Proceed? [y/N] ")
        if not ($ans | str downcase | str starts-with "y") {
            print "Aborted."
            exit 1
        }
    }

    for item in $ITEMS {
        # `security delete-generic-password` is the macOS keychain CLI.
        # Use it directly (mirroring email:secrets:generate, which uses
        # `security add-generic-password`). `fnox` only manipulates the
        # config file, not the keychain items themselves.
        let result = (do { ^security delete-generic-password -s fnox -a $item } | complete)
        if $result.exit_code == 0 {
            print $"deleted: ($item)"
        } else {
            print $"absent:  ($item)"
        }
    }

    print ""
    print "Done. fnox.toml itself is unchanged -- the env-var-to-keychain mapping"
    print "stays in place, so a future 'email:secrets:generate' will repopulate."
}
