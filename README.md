
<h1>Kaspa On Rust</h1>

Welcome to the Rust-based implementation of the Kaspa full-node and its ancillary libraries. This Alpha release serves as a drop-in replacement to the established <a href="https://github.com/kaspanet/kaspad">Golang node</a> (once the rust rewrite is completed), introducing developers to the possibilities of Rust in the Kaspa network's context.

We invite developers and blockchain enthusiasts to collaborate, test, and optimize our Rust implementation. Each line of code here is an opportunity to contribute to the open-source blockchain movement, shaping a platform designed for scalability and speed without compromising on decentralization.

Your feedback, contributions, and issue reports will be integral to evolving this codebase from its Alpha phase into a mature and reliable node in the Kaspa network.

## Installation
  <details>
  <summary>Building on Linux</summary>
  
  1. Install general prerequisites

      ```bash
      sudo apt install curl git build-essential libssl-dev pkg-config 
      ```

  2. Install Protobuf (required for gRPC)
  
      ```bash
      sudo apt install protobuf-compiler libprotobuf-dev #Required for gRPC
      ```
  3. Install the clang toolchain (required for RocksDB and WASM secp256k1 builds)

      ```bash
      sudo apt-get install clang-format clang-tidy \
      clang-tools clang clangd libc++-dev \
      libc++1 libc++abi-dev libc++abi1 \
      libclang-dev libclang1 liblldb-dev \
      libllvm-ocaml-dev libomp-dev libomp5 \
      lld lldb llvm-dev llvm-runtime \
      llvm python3-clang
      ```
  3. Install the [rust toolchain](https://rustup.rs/)
     
     If you already have rust installed, update it by running: `rustup update` 
  4. Install wasm-pack
      ```bash
      cargo install wasm-pack
      ```
  4. Install wasm32 target
      ```bash
      rustup target add wasm32-unknown-unknown
      ```      
  5. Clone the repo
      ```bash
      git clone https://github.com/kaspanet/rusty-kaspa
      cd rusty-kaspa
      ```
  </details>



  <details>  
  <summary>Building on Windows</summary>


  1. [Install Git for Windows](https://gitforwindows.org/) or an alternative Git distribution.

  2. Install [Protocol Buffers](https://github.com/protocolbuffers/protobuf/releases/download/v21.10/protoc-21.10-win64.zip) and add the `bin` directory to your `Path`

  
3. Install [LLVM-15.0.6-win64.exe](https://github.com/llvm/llvm-project/releases/download/llvmorg-15.0.6/LLVM-15.0.6-win64.exe)

    Add the `bin` directory of the LLVM installation (`C:\Program Files\LLVM\bin`) to PATH
    
    set `LIBCLANG_PATH` environment variable to point to the `bin` directory as well

    **IMPORTANT:** Due to C++ dependency configuration issues, LLVM `AR` installation on Windows may not function correctly when switching between WASM and native C++ code compilation (native `RocksDB+secp256k1` vs WASM32 builds of `secp256k1`). Unfortunately, manually setting `AR` environment variable also confuses C++ build toolchain (it should not be set for native but should be set for WASM32 targets). Currently, the best way to address this, is as follows: after installing LLVM on Windows, go to the target `bin` installation directory and copy or rename `LLVM_AR.exe` to `AR.exe`.
  
  4. Install the [rust toolchain](https://rustup.rs/)
     
     If you already have rust installed, update it by running: `rustup update` 
  5. Install wasm-pack
      ```bash
      cargo install wasm-pack
      ```
  6. Install wasm32 target
      ```bash
      rustup target add wasm32-unknown-unknown
      ```      
  7. Clone the repo
      ```bash
      git clone https://github.com/kaspanet/rusty-kaspa
      cd rusty-kaspa
      ```
 </details>      


  <details>  
  <summary>Building on Mac OS</summary>


  1. Install Protobuf (required for gRPC)
      ```bash
      brew install protobuf
      ```
  2. Install llvm. 
  
      The default XCode installation of `llvm` does not support WASM build targets.
To build WASM on MacOS you need to install `llvm` from homebrew (at the time of writing, the llvm version for MacOS is 16.0.1).
      ```bash
      brew install llvm
      ```

      **NOTE:** Homebrew can use different keg installation locations depending on your configuration. For example:
      - `/opt/homebrew/opt/llvm` -> `/opt/homebrew/Cellar/llvm/16.0.1`
      - `/usr/local/Cellar/llvm/16.0.1`

      To determine the installation location you can use `brew list llvm` command and then modify the paths below accordingly:
      ```bash
      % brew list llvm
      /usr/local/Cellar/llvm/16.0.1/bin/FileCheck
      /usr/local/Cellar/llvm/16.0.1/bin/UnicodeNameMappingGenerator
      ...
      ```
      If you have `/opt/homebrew/Cellar`, then you should be able to use `/opt/homebrew/opt/llvm`.

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
  3. Install the [rust toolchain](https://rustup.rs/)
     
     If you already have rust installed, update it by running: `rustup update` 
  4. Install wasm-pack
      ```bash
      cargo install wasm-pack
      ```
  4. Install wasm32 target
      ```bash
      rustup target add wasm32-unknown-unknown
      ```      
  5. Clone the repo
      ```bash
      git clone https://github.com/kaspanet/rusty-kaspa
      cd rusty-kaspa
      ```

 </details>   

  <details>

  <summary>Building WASM framework</summary>

  Rust WebAssembly (Wasm) refers to the use of the Rust programming language to write code that can be compiled into WebAssembly, a binary instruction format that runs in web browsers. This allows for easy development using JS/TS while retaining the benefits of Rust.

  The library can be build in for `NodeJS`, `React Native` and as an `ES6 Module`

  <details>

  <summary>
  NodeJS
  </summary>

  ```bash
  cd rusty-kaspa
  cd wasm
  ./build-node
  cd nodejs
  npm install
  ```

  </details>

  <details>

  <summary>
  ES6
  </summary>

  ```bash
  cd rusty-kaspa
  cd wasm
  ./build-web
  ```

  </details>

  <br>
  This will produce a folder: "nodejs", "web" or "react-native" library in `/wasm` directory depending on your selection.



  </details>

<details>

<summary>
Wallet CLI
</summary>



```bash
cd cli
cargo run --release
```

Wallet CLI is now available via the `/cli` or `/kos` projects.
For KOS, please see [`kos/README.md`](kos/README.md)

</details>



<details>

<summary>
Local Web Wallet
</summary>

Run an http server inside of `wallet/wasm/web` folder. If you don't have once, you can use the following:

```bash
cd wallet/wasm/web
cargo install basic-http-server
basic-http-server
```
The *basic-http-server* will serve on port 4000 by default, so open your web browser and load http://localhost:4000

The framework is compatible with all major desktop and mobile browsers.


</details>


## Running the node

  **Start a mainnet node**

  ```bash
  cargo run --release --bin kaspad
  ```
  **Start a testnet node**

  ```bash
cargo run --release --bin kaspad -- --testnet
  ```

<details>

  <summary>
Using a configuration file
  </summary>

  ```bash
cargo run --release --bin kaspad -- --configfile /path/to/configfile.toml
# or
cargo run --release --bin kaspad -- -C /path/to/configfile.toml
  ```
  - The config file should be a list of \<CLI argument\> = \<value\> separated by newlines. 
  - Whitespace around the `=` is fine, `arg=value` and `arg = value` are both parsed correctly.
  - Values with special characters like `.` or `=` will require quoting the value i.e \<CLI argument\> = "\<value\>".
  - Arguments with multiple values should be surrounded with brackets like `addpeer = ["10.0.0.1", "1.2.3.4"]`.

  For example:
  ```
testnet = true
utxoindex = false
disable-upnp = true
perf-metrics = true
appdir = "some-dir"
netsuffix = 11
addpeer = ["10.0.0.1", "1.2.3.4"]
  ```
 Pass the `--help` flag to view all possible arguments

  ```bash
cargo run --release --bin kaspad -- --help
  ```
</details>

<details>

  <summary>
wRPC
  </summary>

  wRPC subsystem is disabled by default in `kaspad` and can be enabled via:


  JSON protocol:
  ```bash
  --rpclisten-json = <interface:port>
  ```

  Borsh protocol:
  ```bash
  --rpclisten-borsh = <interface:port>
  ```

  **Sidenote:**

  Rusty Kaspa integrates an optional wRPC
  subsystem. wRPC is a high-performance, platform-neutral, Rust-centric, WebSocket-framed RPC 
  implementation that can use [Borsh](https://borsh.io/) and JSON protocol encoding.

  JSON protocol messaging 
  is similar to JSON-RPC 1.0, but differs from the specification due to server-side 
  notifications.

  [Borsh](https://borsh.io/) encoding is meant for inter-process communication. When using [Borsh](https://borsh.io/)
  both client and server should be built from the same codebase.  

  JSON protocol is based on 
  Kaspa data structures and is data-structure-version agnostic. You can connect to the
  JSON endpoint using any WebSocket library. Built-in RPC clients for JavaScript and
  TypeScript capable of running in web browsers and Node.js are available as a part of
  the Kaspa WASM framework.

  **wRPC to gRPC Proxy is deprecated and no longer supported.**

</details>



<details>

<summary>
Mining
</summary>

Mining is currently supported only on testnet, so once you've setup a test node, follow these instructions.

1. Download and unzip the latest binaries bundle of [kaspanet/kaspad](https://github.com/kaspanet/kaspad/releases).

2. In a separate terminal run the kaspanet/kaspad miner:

    ```
    kaspaminer --testnet --miningaddr kaspatest:qrcqat6l9zcjsu7swnaztqzrv0s7hu04skpaezxk43y4etj8ncwfk308jlcew
    ```

    This will create and feed a DAG with the miner getting block templates from the node and submitting them back when mined. The node processes and stores the blocks while applying all currently implemented logic. Execution can be stopped and resumed, the data is persisted in a database.

    You can replace the above mining address with your own address by creating one as described [here](https://github.com/kaspanet/docs/blob/main/Getting%20Started/Full%20Node%20Installation.md#creating-a-wallet-optional). 

</details>


## Benchmarking & Testing


<details> 

<summary>Simulation framework (Simpa)</summary>

Logging in `kaspad` and `simpa` can be [filtered](https://docs.rs/env_logger/0.10.0/env_logger/#filtering-results) by either:

The current codebase supports a full in-process network simulation, building an actual DAG over virtual time with virtual delay and benchmarking validation time (following the simulation generation). 

To see the available commands
```bash 
cargo run --release --bin simpa -- --help
``` 

The following command will run a simulation to produce 1000 blocks with communication delay of 2 seconds and 8 BPS (blocks per second) while attempting to fill each block with up to 200 transactions.   

```bash
cargo run --release --bin simpa -- -t=200 -d=2 -b=8 -n=1000
```

</details>




<details> 

<summary>Heap Profiling</summary>

Heap-profiling in `kaspad` and `simpa` can be done by enabling `heap` feature and profile using the `--features` argument

```bash
cargo run --bin kaspad --profile heap --features=heap
```

It will produce `{bin-name}-heap.json` file in the root of the workdir, that can be inspected by the [dhat-viewer](https://github.com/unofficial-mirror/valgrind/tree/master/dhat)

</details>


<details> 

<summary>Tests</summary>


**Run unit and most integration tests**

```bash
cd rusty-kaspa
cargo test --release
// or install nextest and run
```



**Using nextest**

```bash
cd rusty-kaspa
cargo nextest run --release
```



</details>


<details> 

<summary>Benchmarks</summary>

```bash
cd rusty-kaspa
cargo bench
```

</details>

<details> 

<summary>Logging</summary>

Logging in `kaspad` and `simpa` can be [filtered](https://docs.rs/env_logger/0.10.0/env_logger/#filtering-results) by either:

1. Defining the environment variable `RUST_LOG`
2. Adding the --loglevel argument like in the following example:

    ```
    (cargo run --bin kaspad -- --loglevel info,kaspa_rpc_core=trace,kaspa_grpc_core=trace,consensus=trace,kaspa_core=trace) 2>&1 | tee ~/rusty-kaspa.log
    ```
    In this command we set the `loglevel` to `INFO`.

</details>



