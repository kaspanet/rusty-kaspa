# Rothschild - Transaction Generator

Rothschild is a **transaction load generator** for the Kaspa network. It is used for stress-testing and benchmarking by generating high volumes of transactions against a running node.

## Purpose

- Stress-test node performance under heavy transaction load
- Benchmark transaction throughput and block template generation
- Simulate realistic network activity on testnets

## Usage

```bash
cargo run --release --bin rothschild -- --help
```

Rothschild connects to a Kaspa node via gRPC and continuously generates transactions using a funded private key.

> **Note:** This tool is intended for testing and development purposes only. It is not required for normal node operation.
