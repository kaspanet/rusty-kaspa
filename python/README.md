# Kaspa Python SDK
Rusty-Kaspa Python SDK exposes select Rusty-Kaspa source for use in Python applications, allowing Python developers to interact with the Kaspa BlockDAG.

This package is built from Rusty-Kaspa's Rust source code using [PyO3](https://pyo3.rs/v0.20.0/) and [Maturin](https://www.maturin.rs) to build bindings for Python.

> [!IMPORTANT]
> Kaspa Python SDK is currently in Beta (maybe even Alpha in some regards) status. Please use accordingly.

## Features
A goal of this package is to mirror Kaspa's WASM SDK as closely as possible. From both a feature coverage and usage perspective. 

The following main feature categories are currently exposed for use from Python:
- wRPC Client
- Transaction generation
- Key management

This package does not yet fully mirror WASM SDK, gaps mostly exist around wallet functionality. Future work will bring this as close as possible. The ability to read Rusty-Kaspa's RocksDB database from Python is in progress.

## Installing from Source
This package can currently be installed from source.

### Instructions
1. To build the Python SDK from source, you need to have the Rust environment installed. To do that, follow instructions in the [Installation section of Rusty Kaspa README](https://github.com/kaspanet/rusty-kaspa?tab=readme-ov-file#installation).
2. `cd rusty-kaspa/python` to enter Python SDK crate
3. Run `./build-release` script to build source and built (wheel) dists.
4. The resulting wheel (`.whl`) file location will be printed: `Built wheel for CPython 3.x to <filepath>`. The `.whl` file can be copied to another location or machine and installed there with `pip install <.whl filepath>`

### `maturin develop` vs. `maturin build`
For full details, please see `build-release` script, `build-dev` script, and [Maturin](https://www.maturin.rs) documentation.

Build & install in current active virtual env: `maturin develop --release --features py-sdk`

Build source and built (wheel) distributions: `maturin build --release --strip --sdist --features py-sdk`.

## Usage from Python

The Python SDK module name is `kaspa`. The following example shows how to connect an RPC client to Kaspa's PNN (Public Node Network).

```python
import asyncio
from kaspa import Resolver, RpcClient

async def main():
    resolver = Resolver()
    client = RpcClient(resolver)
    print(await client.get_server_info())

if __name__ == "__main__":
    asyncio.run(main())
```

More detailed examples can be found in `./examples`.

## SDK Project Layout
The Python package `kaspa` is built from the `kaspa-python` crate, which is located at `./python`. 

As such, the Rust `kaspa` function in `./python/src/lib.rs` is a good starting point. This function uses PyO3 to add functionality to the package. 
