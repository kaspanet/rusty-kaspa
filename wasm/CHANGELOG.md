### Latest

#### Event Listener API updates
- Event Listener API has been refactored to mimic DOM standard (similar to `addEventListener` / `removeEventListener` available in the browser, but with additional features)
- replace RpcClient.notify() with RpcClient.addEventListener() / RpcClient.removeEventListener()
- addEventListener() calls have been standardized between RPC, UtxoProcessor, Wallet
- You can now register multiple listeners for the same event type and unregister them individually
- A single registration can accept an array of events to listen to e.g. `["open", "close"]`

#### Other updates
- RpcClient events now support `open`, `close` events to signal the RPC connection state
- RPC events now contain `type` and `data` fields (instead of listeners receiving 2 arguments)
- Rename client-side `Beacon` class to `Resolver`
- ITransactionRecord properties now have appropriate interfaces
- ITransactionData serialization fields have changed from (`transaction = {}` to `data = {}`)

### Release 2024-02-19

- Fix large RPC response deserialization errors in NodeJS caused by the default WebSocket frame size limit.
- Fix event processing in UtxoContext
- Renamed XPrivateKey to PrivateKeyGenerator and XPublicKey to PublicKeyGenerator
- Renamed RpcClient.notify() to RpcClient.registerListener() / RpcClient.removeListener()
- Simplify conversion between different key types (XPrv->Keypair, XPrv->XPub->Pubkey, etc)
- Introduced `Beacon` class that provides connectivity to the community-operated public node infrastructure (backed by `kaspa-beacon` load balancer & node status monitor)
- Created TypeScript type definitions across the entire SDK and refactored `RpcClient` class (as well as many other components) to use TypeScript interfaces
- Changed documentation structure to use `typedoc` available as a part of redistributables or online at https://kaspa.aspectron.org/docs/
- Project-wide documentation updates
- Additional self-contained Web Browser examples
- Modified the structure of WASM32 SDK release to include all variants of libraries (both release and dev builds), examples and documentation in a single package.
