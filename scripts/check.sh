#!/bin/bash

set -euo pipefail

deno fmt README.md --check
cargo fmt --check --all
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test
