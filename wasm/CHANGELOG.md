Latest
------------------
- replace RpcClient.notify() with RpcClient.registerListener() / RpcClient.removeListener()
- You can now register multiple listeners for the same event type and unregister them individually
- RpcClient events now support `open`, `close` events to signal the RPC connection state
- RPC events now contain `type` and `data` fields
- Rename client-side `Beacon` class to `Resolver`


Release 2024-02-19
------------------

- Fix large RPC response deserialization errors in NodeJS caused by the default WebSocket frame size limit.
- Fix event processing in UtxoContext
- Renamed XPrivateKey to PrivateKeyGenerator and XPublicKey to PublicKeyGenerator
- Renamed RpcClient.notify() to RpcClient.registerListener() / RpcClient.removeListener()
- Simplify conversion between different key types (XPrv->Keypair, XPrv->XPub->Pubkey, etc)
- Introduced `Beacon` class that provides connectivity to the community-operated public node infrastructure (backed by `kaspa-beacon` load balancer & node status monitor)
- Created TypeScript type definitions across the entire SDK and refactored `RpcClient` class (as well as many other components) to use TypeScript interfaces
- Changed documentation structure to use `typedoc` available as a part of redistributables or online at https://kaspa.aspectron.org/typedoc/
- Project-wide documentation updates
- Additional self-contained Web Browser examples
- Modified the structure of WASM32 SDK release to include all variants of libraries (both release and dev builds), examples and documentation in a single package.
