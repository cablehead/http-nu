#!/bin/bash

# Set up cross-compilation environment for aarch64-apple-darwin
export CC_aarch64_apple_darwin=aarch64-apple-darwin22.4-clang
export CXX_aarch64_apple_darwin=aarch64-apple-darwin22.4-clang++
export AR_aarch64_apple_darwin=aarch64-apple-darwin22.4-ar
export CFLAGS_aarch64_apple_darwin="-fuse-ld=/usr/local/osxcross/target/bin/aarch64-apple-darwin22.4-ld"

# Parse command line arguments
BUILD_MODE=""
if [[ "$1" == "--release" ]]; then
    BUILD_MODE="--release"
    BUILD_TYPE="release"
    echo "Building for aarch64-apple-darwin (release mode)..."
else
    BUILD_TYPE="debug"
    echo "Building for aarch64-apple-darwin (debug mode)..."
fi

# First attempt - this will likely fail due to libproc issue
cargo build --target aarch64-apple-darwin $BUILD_MODE --color always 2>&1 | tee build.log

# Check if libproc error occurred
if grep -q "osx_libproc_bindings.rs.*No such file" build.log; then
    echo "Detected libproc issue, applying fix..."
    
    # Find the libproc source file
    SOURCE_FILE=$(find /root/.cargo/registry/src/index.crates.io-* -name "libproc-*" -type d | head -1)/docs_rs/osx_libproc_bindings.rs
    
    # Find the destination directory
    DEST_DIR=$(find target/aarch64-apple-darwin/$BUILD_TYPE/build/ -name "libproc-*" -type d | head -1)/out
    
    if [[ -f "$SOURCE_FILE" && -d "$DEST_DIR" ]]; then
        echo "Copying $SOURCE_FILE to $DEST_DIR/"
        cp "$SOURCE_FILE" "$DEST_DIR/"
        
        echo "Retrying build..."
        cargo build --target aarch64-apple-darwin $BUILD_MODE --color always
    else
        echo "Error: Could not find source file or destination directory"
        echo "Source: $SOURCE_FILE"
        echo "Dest: $DEST_DIR"
        exit 1
    fi
fi

# Clean up log file
rm -f build.log