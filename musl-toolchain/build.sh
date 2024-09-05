#!/bin/bash
if [ ! -d "$HOME/x-tools" ]; then
  # Install dependencies
  sudo apt-get update
  sudo apt-get install -y autoconf automake musl-dev musl-tools libtool  libtool-bin unzip help2man python3.10-dev gperf bison flex texinfo gawk libncurses5-dev
  
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

  cat defconfig

  # Expand mini config
  ct-ng defconfig
  
  cat .config

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
    cat build.log
  fi
fi

#FILE_PATH=$(find $HOME/x-tools/** -path "*-gcc")


# Update toolchain variables: C compiler, C++ compiler, linker, and archiver
export CC=$HOME/x-tools/x86_64-multilib-linux-musl/bin/x86_64-multilib-linux-musl-gcc
export CXX=$HOME/x-tools/x86_64-multilib-linux-musl/bin/x86_64-multilib-linux-musl-g++
export LD=$HOME/x-tools/x86_64-multilib-linux-musl/bin/x86_64-multilib-linux-musl-ld
export AR=$HOME/x-tools/x86_64-multilib-linux-musl/bin/x86_64-multilib-linux-musl-ar       

# Check if "$HOME/openssl" directory exists from cache
if [ ! -d "$HOME/openssl" ]; then
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

# Mimalloc overrides.
#   find $HOME/.cargo/registry/src/ -type f -path "*/libmimalloc-sys-*/c_src/mimalloc/CMakeLists.txt" | while read FILE_PATH; do
#       echo "Modifying $FILE_PATH"
#       sed -i '/set_property(TARGET mimalloc-static PROPERTY POSITION_INDEPENDENT_CODE ON)/d' "$FILE_PATH"
#   done
#   find $HOME/.cargo/registry/src/ -type f -path "*/libmimalloc-sys-*/c_src/mimalloc/src/alloc-override.c" | while read FILE_PATH; do
#       echo "Modifying $FILE_PATH. Disabling branch and GLIBC check."
#       sed -i 's/#elif (defined(__GNUC__) || defined(__clang__))/#elif 0/' "$FILE_PATH"
#       sed -i 's/#elif defined(__GLIBC__) && defined(__linux__)/#elif defined(__linux__)/' "$FILE_PATH"
#   done