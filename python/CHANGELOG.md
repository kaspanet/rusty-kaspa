## [1.0.1.post2] - 2025-11-13
### Added
- Support for Python 3.14

### Changed
- Specify Python compatibility as >=3.9,<=3.14
- Upgraded crate pyo3 from 0.24.2 to 0.27.1.
- Upgraded crate pyo3-async-runtimes from 0.24 to 0.27.0
- Upgraded crate pyo3-log from 0.12.4 to 0.13.2
- Upgraded crate serde-pyobject from 0.6.2 to 0.8.0
- CI updates


## [1.0.1.post1] - 2025-09-27
### Added
- Added RPC method `submit_block`.
- RPC method `get_virtual_chain_from_block` support of `minConfirmationCount`.
- RPC method doc strings in .pyi with expected `request` dict structure (for calls that require a `request` dict).

### Changed
- RPC method `submit_transaction`'s `request` parameter now supports key `allowOrphan`. A deprecation warning will print when key `allow_orphan` is used. Support for `allow_orphan` will be removed in future version. This moves towards case consistency.
- KeyError is now raised when an expected key is not contained in a dictionary. Prior, a general Exception was raised.
