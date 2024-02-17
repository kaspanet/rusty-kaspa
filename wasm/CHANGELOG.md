Updates since 0.3.14
--------------------

- Fix event processing in UtxoContext
- Renamed XPrivateKey to PrivateKeyGenerator and XPublicKey to PublicKeyGenerator
- Simplify conversion between different key types
- Introduced `Beacon` class that provides connectivity to the community-operated public node infrastructure (backed by `kaspa-beacon` load balancer & node status monitor)
- Created TypeScript type definitions across the entire SDK and refactored `RpcClient` class (as well as many other components) to use TypeScript interfaces
- Changed documentation structure to use `typedoc` available as a part of redistributables or online at https://kaspa.aspectron.org/typedoc/
- Project-wide documentation updates
