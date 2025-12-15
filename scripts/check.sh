#!/bin/bash

set -euo pipefail

nu tests/test_router.nu
nu tests/test_html.nu
deno fmt README.md --check
cargo fmt --check --all
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings -W clippy::uninlined_format_args
cargo test
