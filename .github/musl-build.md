# How to build Rusty-Kaspa on Musl

This guide will show you how to build Rusty-Kaspa on Musl. This guide is intended for developers who are familiar with building software from source.

## Prerequisites

- Rust

### Steps

1. Install `crosstool-ng`

```bash
sudo apt-get install -y git autoconf automake libtool libtool-bin unzip help2man python3.10-dev gperf bison flex texinfo gawk libncurses5-dev
git clone https://github.com/crosstool-ng/crosstool-ng
cd crosstool-ng
./bootstrap
./configure --prefix=SOME_PATH
make
make install
cd ..
```

2. Configure ctng

Use the .config file I made.

Enter the menuconfig: `ct-ng menuconfig`

Make sure the following options are set:
C library -> musl
C++ -> yes
Verbose libstdc++ configuration -> yes

3. Build the toolchain

```bash
ct-ng build
```

It will likely put it into `~/x-tools/x86_64-multilib-linux-musl`

4. Set the environment variables

```bash
export CC=/home/starkbamse/x-tools/x86_64-multilib-linux-musl/bin/x86_64-multilib-linux-musl-gcc
export CXX=/home/starkbamse/x-tools/x86_64-multilib-linux-musl/bin/x86_64-multilib-linux-musl-g++
export LD=/home/starkbamse/x-tools/x86_64-multilib-linux-musl/bin/x86_64-multilib-linux-musl-ld
export AR=/home/starkbamse/x-tools/x86_64-multilib-linux-musl/bin/x86_64-multilib-linux-musl-ar
```

5. Build openssl

```bash
wget https://www.openssl.org/source/openssl-1.1.1l.tar.gz
tar xzf openssl-1.1.1l.tar.gz
cd openssl-1.1.1l
```

```bash
./Configure no-shared --static linux-x86_64 -fPIC --prefix=SOME_PATH_FOR_OPENSSL
make depend
make
make install
```

6. Set configuration for RK

Create a `.cargo/config` file in the root of the project with the following content:

```toml
[target.x86_64-unknown-linux-musl]
linker = "/home/starkbamse/x-tools/x86_64-multilib-linux-musl/bin/x86_64-multilib-linux-musl-gcc"
rustflags = [
  "-C", "link-arg=-static",
  "-C", "linker=/home/starkbamse/x-tools/x86_64-multilib-linux-musl/bin/x86_64-multilib-linux-musl-gcc", 
  "-L", "native=/home/starkbamse/rk-ctng/openssl-build/lib" # <-- SOME_PATH_FOR_OPENSSL
]
```

7. Set OPENSSL_DIR and OPENSSL_STATIC
```bash
export OPENSSL_DIR=/usr/local/musl/
export OPENSSL_STATIC=true
```

8. Build Rusty-Kaspa

```bash
cargo build --release --bin=kaspad --target x86_64-unknown-linux-musl
```

9. Build will fail because of rocksdb error:

This will fix the error: thanks to supertypo
```bash
find ~/.cargo/registry -type d -name "*rocksdb-sys*"
sed -i '1i #include <cstdint>' /home/starkbamse/.cargo/registry/src/index.crates.io-6f17d22bba15001f/librocksdb-sys-0.16.0+8.10.0/rocksdb/options/offpeak_time_info.h
```

10. Build Rusty-Kaspa again

```bash
cargo build --release --bin=kaspad --target x86_64-unknown-linux-musl
```
bye bye zigbuild :)
