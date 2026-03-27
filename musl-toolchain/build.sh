#!/bin/bash
set -euo pipefail

# Tag name for the pre-built toolchain release
TOOLCHAIN_TAG="musl-toolchain-v1"

# Calculate the hash of the preset file
CURRENT_PRESET_HASH=$(sha256sum $GITHUB_WORKSPACE/musl-toolchain/preset.sh | awk '{print $1}')
PRESET_HASH_FILE="$HOME/x-tools/preset_hash"

echo "Current preset hash: $CURRENT_PRESET_HASH"

# Traverse to working directory
cd $GITHUB_WORKSPACE/musl-toolchain

# Set the preset
source preset.sh

# Check if the toolchain is already installed and up-to-date
if [ -d "$HOME/x-tools" ] && [ -f "$PRESET_HASH_FILE" ] && [ "$(cat $PRESET_HASH_FILE)" = "$CURRENT_PRESET_HASH" ]; then
  echo "Toolchain already installed and up-to-date, skipping"
else
  echo "Toolchain not found or outdated, downloading pre-built toolchain from release..."
  rm -rf "$HOME/x-tools"

  gh release download "$TOOLCHAIN_TAG" \
    --repo "$GITHUB_REPOSITORY" \
    --pattern "x-tools.tar.zst" \
    --dir /tmp

  echo "Extracting pre-built toolchain..."
  tar --use-compress-program=zstd -xf /tmp/x-tools.tar.zst -C "$HOME"
  rm /tmp/x-tools.tar.zst

  # Verify the downloaded toolchain matches current preset
  if [ ! -f "$PRESET_HASH_FILE" ] || [ "$(cat $PRESET_HASH_FILE)" != "$CURRENT_PRESET_HASH" ]; then
    echo "ERROR: Pre-built toolchain preset hash mismatch."
    echo "Expected: $CURRENT_PRESET_HASH"
    echo "Got:      $(cat $PRESET_HASH_FILE 2>/dev/null || echo 'missing')"
    echo "Run the 'Build musl toolchain' workflow to rebuild the release."
    exit 1
  fi

  echo "Pre-built toolchain matches current preset, ready to use"
fi

# Update toolchain variables: C compiler, C++ compiler, linker, and archiver
export CC=$HOME/x-tools/$CTNG_PRESET/bin/$CTNG_PRESET-gcc
export CXX=$HOME/x-tools/$CTNG_PRESET/bin/$CTNG_PRESET-g++
export LD=$HOME/x-tools/$CTNG_PRESET/bin/$CTNG_PRESET-ld
export AR=$HOME/x-tools/$CTNG_PRESET/bin/$CTNG_PRESET-ar

# Exports for cc crate
# https://docs.rs/cc/latest/cc/#external-configuration-via-environment-variables
export RANLIB_x86_64_unknown_linux_musl=$HOME/x-tools/$CTNG_PRESET/bin/$CTNG_PRESET-ranlib
export CC_x86_64_unknown_linux_musl=$CC
export CXX_x86_64_unknown_linux_musl=$CXX
export AR_x86_64_unknown_linux_musl=$AR
export LD_x86_64_unknown_linux_musl=$LD

# Set environment variables for static linking
export OPENSSL_STATIC=true
export RUSTFLAGS="-C link-arg=-static"

# We specify the compiler that will invoke linker
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=$CC

# Add target
rustup target add x86_64-unknown-linux-musl

# Install missing dependencies
cargo fetch --target x86_64-unknown-linux-musl
