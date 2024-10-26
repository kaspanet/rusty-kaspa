# Python bindings for Rusty Kaspa 
Rusty-Kaspa/Rust bindings for Python, using [PyO3](https://pyo3.rs/v0.20.0/) and [Maturin](https://www.maturin.rs). The result is a Python package that exposes rusty-kaspa/rust source for use in Python programs.

# Building from Source
1. Ensure Python 3.9 or higher (`python --version`) is installed.
2. Clone Python SDK source: `git clone -b python  https://github.com/aspectron/rusty-kaspa.git`
3. `cd rusty-kaspa` 
4. `cd python` (Python SDK crate)
5. Create Python virtual environment with `python -m venv env` or your preferred env tool.
6. Activate Python virtual environment: 
- Unix-like systems: `source env/bin/activate`
- Windows: `env/scripts/activate.bat`
5. Install Maturin build tool: `pip install maturin`
6. Build Python package with Maturin:
- Build & install in current active virtual env: `maturin develop --release --features py-sdk`
- Build source and built (wheel) distributions: `maturin build --release --strip --sdist --features py-sdk`. The resulting wheel (.whl file) location will be printed `Built wheel for CPython 3.x to <filepath>`. The `.whl` file can be copied to another location or machine and installed there with `pip install <.whl filepath>`

# Usage from Python

The Python SDK module name is `kaspa`. The following example shows how to connect an RPC client to Kaspa's PNN (Public Node Network).

```python
import asyncio
from kapsa import Resolver, RpcClient

async def main():
    resolver = Resolver()
    client = RpcClient(resolver)
    print(await client.get_server_info())

if __name__ == "__main__":
    asyncio.run(main())
```

More detailed examples can be found in `./examples`.

# Project Layout
The Python package `kaspa` is built from the `kaspa-python` crate, which is located at `./python`. 

As such, the `kaspa` function in `./python/src/lib.rs` is a good starting point. This function uses PyO3 to add functionality to the package. 
