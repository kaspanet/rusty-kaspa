#!/bin/bash

PRESET_HASH_FILE="$HOME/x-tools/preset_hash"
# Calculate the hash of the preset file
CURRENT_PRESET_HASH=$(sha256sum $GITHUB_WORKSPACE/musl-toolchain/preset.sh | awk '{print $1}')

echo "Current preset hash: $CURRENT_PRESET_HASH"

# If the toolchain is not installed or the preset has changed or the preset hash file does not exist
if [ ! -d "$HOME/x-tools" ] || [ ! -f "$PRESET_HASH_FILE" ] || [ "$(cat $PRESET_HASH_FILE)" != "$CURRENT_PRESET_HASH" ]; then
  # Install dependencies
  sudo apt-get update
  sudo apt-get install -y autoconf automake libtool  libtool-bin unzip help2man python3.10-dev gperf bison flex texinfo gawk libncurses5-dev
  
  # Clone crosstool-ng
  git clone https://github.com/crosstool-ng/crosstool-ng
  
  # Configure and build crosstool-ng
  cd crosstool-ng
  # Use version 1.26
  git checkout crosstool-ng-1.26.0
  ./bootstrap
  ./configure --prefix=$HOME/ctng
  make
  make install
  # Add crosstool-ng to PATH
  export PATH=$HOME/ctng/bin:$PATH
  # Configure and build the musl toolchain
  cd $GITHUB_WORKSPACE/musl-toolchain

  source preset.sh

  # Expand mini config
  ct-ng $CTNG_PRESET
  
  # Build the toolchain
  ct-ng build > build.log 2>&1
  
  # Set status to the exit code of the build
  status=$?
  
  # We store the log in a file because it bloats the screen too much
  # on GitHub Actions. We print it only if the build fails.
  echo "Build result:"
  if [ $status -eq 0 ]; then
    echo "Build succeeded"
    ls -la $HOME/x-tools
  else
    echo "Build failed, here's the log:"
    cat .config
    cat build.log
  fi
fi

# Update toolchain variables: C compiler, C++ compiler, linker, and archiver
export CC=$HOME/x-tools/$CTNG_PRESET/bin/$CTNG_PRESET-gcc
export CXX=$HOME/x-tools/$CTNG_PRESET/bin/$CTNG_PRESET-g++
export LD=$HOME/x-tools/$CTNG_PRESET/bin/$CTNG_PRESET-ld
export AR=$HOME/x-tools/$CTNG_PRESET/bin/$CTNG_PRESET-ar       

# Check if "$HOME/openssl" directory exists from cache
if [ ! -d "$HOME/x-tools" ] || [ ! -f "$PRESET_HASH_FILE" ] || [ "$(cat $PRESET_HASH_FILE)" != "$CURRENT_PRESET_HASH" ]; then
  wget https://www.openssl.org/source/openssl-1.1.1l.tar.gz
  tar xzf openssl-1.1.1l.tar.gz
  cd openssl-1.1.1l
  # Configure OpenSSL for static linking
  ./Configure no-shared --static linux-x86_64 -fPIC --prefix=$HOME/openssl
  make depend
  make
  make install
  # Check if OpenSSL was installed successfully
  ls -la $HOME/openssl

  # Store the current hash of preset.sh after successful build
  echo "$CURRENT_PRESET_HASH" > "$PRESET_HASH_FILE"
fi

# Set environment variables for static linking
export OPENSSL_DIR=$HOME/openssl
export OPENSSL_STATIC=true
export RUSTFLAGS="-C link-arg=-static -C link-arg=-L$OPENSSL_DIR/lib"
# We specify the compiler that will invoke linker
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=$CC
# Add target
rustup target add x86_64-unknown-linux-musl
# Install missing dependencies
cargo fetch --target x86_64-unknown-linux-musl
# Patch missing include in librocksdb-sys-0.16.0+8.10.0. Credit: @supertypo
FILE_PATH=$(find $HOME/.cargo/registry/src/ -path "*/librocksdb-sys-0.16.0+8.10.0/*/offpeak_time_info.h")
if [ -n "$FILE_PATH" ]; then
  sed -i '1i #include <cstdint>' "$FILE_PATH"
else
  echo "No such file for sed modification."
fi