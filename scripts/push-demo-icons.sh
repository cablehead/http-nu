#!/usr/bin/env bash
# Generate PWA icons from the SVG sources via resvg (192, 512, 512-maskable,
# apple-touch-180). Called by `mise run push-demo:icons`, which provides resvg
# on PATH (per-task tool). Kept as a bash script so the mise task body stays a
# single nu-parseable command (ci:check-nu parses task run bodies as nushell).
set -e
cd "$(dirname "$0")/.."

mkdir -p examples/push-demo/www/icons
resvg -w 192 examples/push-demo/icons.svg          examples/push-demo/www/icons/192.png
resvg -w 512 examples/push-demo/icons.svg          examples/push-demo/www/icons/512.png
resvg -w 512 examples/push-demo/icons-maskable.svg examples/push-demo/www/icons/512-maskable.png
resvg -w 180 examples/push-demo/icons.svg          examples/push-demo/www/icons/apple-touch-180.png
echo "Icons regenerated."
