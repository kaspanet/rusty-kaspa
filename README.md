# Kaspa on Rust

Work in progress to implement the Kaspa full-node and related libraries in the Rust programming language.

## Getting started

- Install Protobuf (required for grpc)
  - Linux: `sudo apt install protobuf-compiler libprotobuf-dev`
  - Windows: [protoc-21.10-win64.zip](https://github.com/protocolbuffers/protobuf/releases/download/v21.10/protoc-21.10-win64.zip) and add `bin` dir to `Path`
  - MacOS: `brew install protobuf`
- Install the [clang toolchain](https://clang.llvm.org/) (required for RocksDB)
  - Linux: `sudo apt intall clang`
  - Windows: [LLVM-15.0.6-win64.exe](https://github.com/llvm/llvm-project/releases/download/llvmorg-15.0.6/LLVM-15.0.6-win64.exe) and set `LIBCLANG_PATH` env var pointing to the `bin` dir of the llvm installation
  - MacOS: Please see [Installing clang toolchain on MacOS](#installing-clang-toolchain-on-macos)
- Install the [rust toolchain](https://rustup.rs/)
- Run the following commands:

```bash
$ git clone https://github.com/kaspanet/rusty-kaspa
$ cd rusty-kaspa
```

## Experimenting with the node

The `kaspad` rust executable is currently at the initial stage where a devnet consensus instance can be built and mined locally through the RPC interface. The P2P network is not supported yet. To see it in action, perform the following:

```bash
$ cargo run --bin kaspad --release
```

- Download and unzip the latest binaries bundle of [kaspanet/kaspad](https://github.com/kaspanet/kaspad/releases).

- In a separate terminal run the kaspanet/kaspad miner:

```bash
$ kaspaminer --rpcserver 127.0.0.1:16610 --devnet --miningaddr kaspadev:qrcqat6l9zcjsu7swnaztqzrv0s7hu04skpaezxk43y4etj8ncwfkuhy0zmax
```

- This will create and feed a DAG with the miner getting block templates from the node and submitting them back when mined. The node processes and stores the blocks while applying all currently implemented logic. Execution can be stopped and resumed, the data is persisted in a database.

## Simulation framework (Simpa)

Additionally, the current codebase supports a full in-process network simulation, building an actual DAG over virtual time with virtual delay and benchmarking validation time (following the simulation generation). Execute 
```bash 
cargo run --release --bin simpa -- --help
``` 
to see the full command line configuration supported by `simpa`. For instance, the following command will run a simulation producing 1000 blocks with communication delay of 2 seconds and BPS=8, and attempts to fill each block with up to 200 transactions.   

```bash
$ cargo run --release --bin simpa -- -t=200 -d=2 -b=8 -n=1000
```

## Logging

Logging in `kaspad` and `simpa` can be [filtered](https://docs.rs/env_logger/0.10.0/env_logger/#filtering-results) either by defining the environment variable `RUST_LOG` and/or by adding a `--loglevel` argument to the command, ie.:

```bash
$ cargo run --bin kaspad -- --loglevel info,kaspa_rpc_core=trace,kaspa_grpc_core=trace,consensus=trace,kaspa_core=trace
```



## Tests & Benchmarks

- To run all current tests use:

```bash
$ cd rusty-kaspa
$ cargo test --release
// or install nextest and run
$ cargo nextest run --release
```

- To run current benchmarks:

```bash
$ cd rusty-kaspa
$ cargo bench
```

## Building WASM

To build rusty-kaspa wasm library, do the following:

```bash
cd wasm
./build-web
```
This will produce a wasm library in `/web-root` directory

## Installing clang toolchain on MacOS

The default XCode installation of `llvm` does not support WASM build targets.
To build WASM on MacOS you need to install `llvm` from homebrew (at the time of writing MacOS version is 13.0.1).

```bash
brew install llvm
```
NOTE: depending on your setup, the installation location may be different.
To determine the installation location you can type `which llvm` or `which clang`
and then modify the paths below accordingly.

Add the following to your `~/.zshrc` file:
```bash
export PATH="/opt/homebrew/opt/llvm/bin:$PATH"
export LDFLAGS="-L/opt/homebrew/opt/llvm/lib"
export CPPFLAGS="-I/opt/homebrew/opt/llvm/include"
export AR=/opt/homebrew/opt/llvm/bin/llvm-ar
```
Reload the `~/.zshrc` file
```bash
source ~/.zshrc
```