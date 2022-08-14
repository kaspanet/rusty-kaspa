# Kaspa on Rust  


Work in progress to implement the Kaspa full-node and related libraries in the Rust programming language. 

## Getting started 

- Install the [rust toolchain](https://rustup.rs/).

- Run the following commands:
```bash
$ git clone https://github.com/kaspanet/rusty-kaspa
$ cd rusty-kaspa/kaspad
$ cargo run --release
```

- This will run a short simulation producing a random DAG and processing it (applying all currently implemented logic). 

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
