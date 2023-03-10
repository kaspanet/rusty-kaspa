# Kaspa on Rust

Work in progress to implement the Kaspa full-node and related libraries in the Rust programming language.

## Getting started

- Install Protobuf (required for grpc)
  - Linux: `sudo apt install protobuf-compiler libprotobuf-dev`
  - Windows: [protoc-21.10-win64.zip](https://github.com/protocolbuffers/protobuf/releases/download/v21.10/protoc-21.10-win64.zip) and add `bin` dir to `Path`
  - MacOS: `brew install protobuf`
- Install the [clang toolchain](https://clang.llvm.org/) (required for RocksDB)
  - Linux: `sudo apt install clang`
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
$ cargo run --bin kaspad --release -- --devnet
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

## JSON and Borsh RPC protocols

In addition to gRPC, Rusty Kaspa integrates an optional wRPC
subsystem. wRPC is a high-performance, platform-neutral, Rust-centric, WebSocket-framed RPC 
implementation that can use Borsh and JSON protocol encoding. JSON protocol messaging 
is similar to JSON-RPC 1.0, but differs from the specification due to server-side 
notifications. Borsh encoding is meant for inter-process communication. When using Borsh
both client and server should be built from the same codebase.  JSON protocol is based on 
Kaspa data structures and is data-structure-version agnostic. You can connect to the
JSON endpoint using any WebSocket library. Built-in RPC clients for JavaScript and
TypeScript capable of running in web browsers and Node.js are available as a part of
the Kaspa WASM framework.

## Enabling wRPC

wRPC subsystem is disabled by default in `kaspad` and can be enabled via:
- `--rpclisten-json = <interface:port>` for JSON protocol
- `--rpclisten-borsh = <interface:port>` for Borsh protocol

## wRPC to gRPC Proxy

Proxy providing wRPC to gRPC relay is available in `rpc/wrpc/proxy`.
By default, the proxy server will connect to *grpc://127.0.01:16110*
while offering wRPC connections on *wrpc://127.0.0.1:17110*. Use `--help`
to see configuration options.

The Proxy server is currently used for testing with the Golang implementation
of Kaspad. At the time of writing, the gRPC has only partial messsage translation
implementation.

To run the proxy:
```bash
cd rpc/wrpc/proxy
cargo run
```

## Native JavaScript & TypeScript RPC clients for Browsers and Node.js environments

Integration in a Browser and Node.js environments is possible using WASM.
The JavaScript code is agnostic to which environment it runs in.
NOTE: to run in Node.js environment, you must instantiate a W3C WebSocket
shim using a `WebSocket` crate before initializing Kaspa environment:
`globalThis.WebSocket = require('websocket').w3cwebsocket;`

Prerequisites:
- WasmPack: https://rustwasm.github.io/wasm-pack/installer/

To test Node.js:
- Make sure you have Rust and WasmPack installed
- Start Golang Kaspad
- Start wRPC proxy

```bash
cd rpc/wrpc/wasm
./build-node
cd nodejs
npm install
node index
```

You can take a look at `rpc/wrpc/wasm/nodejs/index.js` to see the use of the native JavaScript & TypeScript APIs.

NOTE: `npm install` is needed to install [WebSocket](https://github.com/theturtle32/WebSocket-Node) module.
When running in the Browser environment, no additional dependencies are necessary because
the browser provides the W3C WebSocket class natively.

## Wallet CLI

The wallet CLI is under heavy development.  To test the environment you can do the following:

- Start Golang Kaspad
- Start wRPC proxy

Native (OS command line):

```bash
cd wallet/native
cargo run
```

Web Browser (WASM):

Run an http server inside of `wallet/wasm/web` folder. If you don't have once, you can use `basic-http-server`.
```bash
cd wallet/wasm/web
cargo install basic-http-server
basic-http-server
```
The *basic-http-server* will serve on port 4000 by default, so open your web browser and load http://localhost:4000

The framework is compatible with all major desktop and mobile browsers.

## Using Wallet CLI

This project is under heavy development and currently demonstrates only the RPC data exchange and
address generation using the native or WASM wallet library. 

Here are the few commands that work:

```
get-info
subscribe-daa-score
new-address
exit
```
