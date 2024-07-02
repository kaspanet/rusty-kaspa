# Python bindings for Rusty Kaspa 
Rusty-Kaspa/Rust bindings for Python, using [PyO3](https://pyo3.rs/v0.20.0/) and [Maturin](https://www.maturin.rs). The result is a Python package that exposes rusty-kaspa/rust source for use in Python programs.

# Building from Source
1. Ensure Python 3.8 or higher (`python --version`) is installed. [*TODO validate 3.8 or higher is correct*]. Python installers can be found on [python.org](https://www.python.org).
2. `cd ./python` 
3. Create Python virtual environment: `python -m venv env`
4. Activate Python virtual env: 
- Unix-based systems: `source env/bin/activate`
- Windows: `env/scripts/activate.bat`
5. Install `maturin` build tool: `pip install maturin`
6. Build Python package with Maturin:
- For local development, build and install in active Python virtual env: `maturin develop --release --features py-sdk`
- To build source and built (wheel) distributions: `maturin build --release --strip --sdist --features py-sdk`

# Usage from Python
See Python files in `./python/examples`.

# Project Layout
The Python package `kaspapy` is built from the `kaspa-python` crate, which is located at `./python`. 

As such, the `kaspapy` function in `./python/src/lib.rs` is a good starting point. This function uses PyO3 to add functionality to the package. 
