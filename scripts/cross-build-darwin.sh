#!/bin/bash
set -euo pipefail

# Set up cross-compilation environment for aarch64-apple-darwin
export CC_aarch64_apple_darwin=aarch64-apple-darwin22.4-clang
export CXX_aarch64_apple_darwin=aarch64-apple-darwin22.4-clang++
export AR_aarch64_apple_darwin=aarch64-apple-darwin22.4-ar
export CFLAGS_aarch64_apple_darwin="-fuse-ld=/usr/local/osxcross/target/bin/aarch64-apple-darwin22.4-ld"

# Parse command line arguments
BUILD_MODE=""
BUILD_TYPE="debug"
if [[ "${1:-}" == "--release" ]]; then
    BUILD_MODE="--release"
    BUILD_TYPE="release"
fi
echo "Building for aarch64-apple-darwin ($BUILD_TYPE mode)..."

build() {
    cargo build --target aarch64-apple-darwin $BUILD_MODE --color always "$@"
}

# First attempt - may fail due to the libproc bindgen cross-compile issue,
# where build.rs cannot emit osx_libproc_bindings.rs from a Linux host.
# Capture the real cargo exit status through the tee pipe.
set +e
build 2>&1 | tee build.log
status=${PIPESTATUS[0]}
set -e

# Detect the libproc case by the missing bindings file, independent of the
# exact rustc wording ("No such file" vs "couldn't find file"), but only when
# the build actually failed.
if [[ $status -ne 0 ]] && grep -qE "osx_libproc_bindings\.rs" build.log; then
    echo "Detected libproc bindings issue, applying fix..."

    SOURCE_FILE=$(find /root/.cargo/registry/src/index.crates.io-* -name "libproc-*" -type d | head -1)/docs_rs/osx_libproc_bindings.rs
    DEST_DIR=$(find target/aarch64-apple-darwin/$BUILD_TYPE/build/ -name "libproc-*" -type d | head -1)/out

    if [[ -f "$SOURCE_FILE" && -d "$DEST_DIR" ]]; then
        echo "Copying $SOURCE_FILE to $DEST_DIR/"
        cp "$SOURCE_FILE" "$DEST_DIR/"
        echo "Retrying build..."
        build
    else
        echo "Error: could not find libproc bindings source or destination" >&2
        echo "Source: $SOURCE_FILE" >&2
        echo "Dest:   $DEST_DIR" >&2
        exit 1
    fi
elif [[ $status -ne 0 ]]; then
    echo "Build failed for a reason other than the libproc bindings issue" >&2
    exit "$status"
fi

rm -f build.log

# Guard against a silent failure slipping through as a green build: the
# packaging step tars whatever is present, so make sure the binary is here.
BIN="target/aarch64-apple-darwin/$BUILD_TYPE/http-nu"
if [[ ! -x "$BIN" ]]; then
    echo "Error: expected binary not found at $BIN" >&2
    exit 1
fi
echo "Built $BIN"
