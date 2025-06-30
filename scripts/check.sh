#!/bin/bash

set -euo pipefail

cargo fmt --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test
