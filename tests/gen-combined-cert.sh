#!/usr/bin/env bash
# Regenerates the self-signed TLS test fixtures in this directory:
# cert.pem, key.pem, and combined.pem (cert.pem + key.pem concatenated).
#
# Used by server_test.rs: combined.pem is passed to `http-nu --tls`, and
# cert.pem is used as the curl --cacert to trust it. Run this from the repo
# root whenever the cert expires (openssl x509 -in tests/cert.pem -noout
# -dates):
#
#   ./tests/gen-combined-cert.sh

set -euo pipefail
cd "$(dirname "$0")"

openssl req -x509 -newkey rsa:4096 -sha256 -days 36500 -nodes \
    -keyout key.pem -out cert.pem \
    -subj "/CN=localhost" \
    -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"

cat cert.pem key.pem > combined.pem

echo "Regenerated cert.pem, key.pem, combined.pem"
openssl x509 -in cert.pem -noout -dates
