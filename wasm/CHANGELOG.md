Latest online documentation available at: https://kaspa.aspectron.org/docs/

### Latest Release

- Replace `MassCalculator` with `calculateTransactionMass` and `calculateTransactionFee` functions.
- Change `createTransaction` function signature (remove requirement for change address).
- Make `ITransactionInput.signatureScript` optional (if not supplied, the signatureScript is assigned an empty vector).

### Release 2024-07-17

- Fix issues with deserializing manually-created objects matching `IUtxoEntry` interface.
- Allow arguments expecting ScriptPublicKey to receive `{ version, script }` object or a hex string.
- Fix `Transaction::serializeToObject()` return type (now returning `ISerializeTransaction` interface).
- Adding `setUserTransactionMaturityDAA()` and `setCoinbaseTransactionMaturityDAA()` that allow customizing
the maturity DAA periods for user and coinbase transactions.

### Release 2024-06-12

- Fix `PublicKeyGenerator::change_address_as_string()` that was returning the receive address.
- WASM SDK now builds as a GitHub artifact during the CI process.
- `State` renamed to `PoW`
- Docs now have a PoW section that unifies all PoW-related classes and functions.
- `TransactionRecord.data` (`TransactionData`) now has correct TypeScript bindings.

### Release 2024-05-26

- Adding utility functions:  `payToAddressScript()`, `payToScriptHashScript()`, `payToScriptHashSignatureScript()`, `addressFromScriptPublicKey()`, `isScriptPayToPubkey()`, `isScriptPayToPubkeyECDSA()`, `isScriptPayToScriptHash()`.
- Adding `UtxoProcessor::isActive` property to check if the processor is in active state (connected and running). This property can be used to validate the processor state before invoking it's functions (that can throw is the UtxoProcessor is offline).
- Rename `UtxoContext::active` to `UtxoContext::isActive` for consistency.

### Release 2024-04-27
 - IAccountsCreateRequest interface simplified by flattering it and now it is union for future expansion for multisig etc.
 - IWalletEvent interface updated for Events with TransactionRecord
 - WIP: wallet api example under wallet/wallet.js
 - Bug fixes: wallet.ensure_default_account, ECDSA address creation methods

### Release 2024-04-17

- Rename RPC "open" and "close" events to "connect" and "disconnect", TypeScript `RpcEventType.Open` and `RpcEventType.Close` enums to `RpcEventType.Connect` and `RpcEventType.Disconnect` (the renaming is done to prevent confusion in other layers of the WASM SDK where "open" and "close" event names represent the Wallet open state).
- `RpcClient.open` boolean state getter renamed to `RpcClient.connected`
- Fix examples missed during `publicKey()->toPublicKey()` rename.

### Release 2024-04-17

- Transaction::addresses() returns a list of unique addresses used by transaction inputs
- PendingTransaction::addresses change from getter to a function
- Address::validate(string) static helper to test address validity

### Release 2024-04-14

- Merge with Rusty Kaspa master (0.13.6)

### Release 2024-04-13

- Change `signTransaction()` to accept `Transaction` instead of `SignableTransaction`
- Remove `SignableTransaction` from the SDK (as `Transaction` now provides all signing functionality)
- Fix a bug in `TransactionRecord` that was returning incorrect getter for `record.data`
- Added `Transaction::addresses()` that returns address list for all UTXOs associated with transaction inputs.
- Fix declarations of events (RpcClient,UtxoProcessor) that do not carry any event data (event data is now declared as `undefined`)

### Release 2024-03-31

- Rename `kaspa-beacon` app to `kaspa-resolver`
- Change RpcClient, UtxoProcessor and Wallet event handlers in typescript to receive typed event data
- UtxoProcessor and Wallet event handlers now deliver TransactionRecord events (Discovery, Pending, etc.)
as Rust or WASM objects, allowing user to call `hasAddress(<address>)` on the received `event.data.record` object.

### Release 2024-03-19

- Fix type checks when passing arrays to transaction `Generator` entries.

### Release 2024-03-14

- Introduce IWASM32BindingsConfig for configuration of class naming when using WASM32 bindings.
- Introduce serializeToJSON for `PendingTransaction` class (deserializable with `Transaction` class).
- Introduce serializeToJSON and deserializeFromJSON methods for `Transaction` class.

### Release 2024-03-11

- Fix `requestAnimationFrame` use in chrome extension environment.
- Add rejection in `Generator` when `priorityFee` is `undefined` while outputs are present.
- Introduce `CryptoBox` class for encryption/decryption of data using public/private keys.
- sha256x and other hash functions now have two variants sha256FromBinary and sha256FromText
- Changed `XPub.publicKey()` to `XPub.toPublicKey()`
- Most functions returning key strings now return `PrivateKey` or `PublicKey`; this allows function chaining `xpub.deriveChild(0).toPublicKey().toAddress(networkId).toString()`
- `PrivateKey` now has `toPublicKey()`, `toAddress()`, `toAddressECDSA()` methods
- Introduce `XOnlyPublicKey` which can be obtained from `PublicKey`: `xpub.toXOnlyPublicKey()` and `xpub.toXOnlyPublicKey().toAddress(networkId)`. 

### Release 2024-02-26

- Add `UtxoProcessor.start()/stop()` methods for explicit start/stop of the `UtxoProcessor` event processing.
- Remove `async` markers from UtxoProcessor and UtxoContext constructors.
- Add `UtxoProcessor.setNetworkId()` method to change the network ID for existing `UtxoProcessor` (`UtxoProcessor` must be stopped before changing the network id).
- Add `UtxoProcessor.networkId` property to get the current network ID.
- Add `UtxoContext.matureLength()` and `matureRange(from,to)` for access to mature UTXO entries.


### Release 2024-02-25

#### Event Listener API updates
- Event Listener API has been refactored to mimic DOM standard (similar to `addEventListener` / `removeEventListener` available in the browser, but with additional features)
- replace `RpcClient.notify()` with `RpcClient.addEventListener()` / `RpcClient.removeEventListener()`
- `addEventListener()` calls have been standardized between RPC, UtxoProcessor, Wallet
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
- Renamed `XPrivateKey` to `PrivateKeyGenerator` and `XPublicKey` to `PublicKeyGenerator`
- Simplify conversion between different key types (`XPrv->Keypair`, `XPrv->XPub->Pubkey`, etc)
- Introduced `Beacon` class that provides connectivity to the community-operated public node infrastructure (backed by `kaspa-beacon` load balancer & node status monitor)
- Created TypeScript type definitions across the entire SDK and refactored `RpcClient` class (as well as many other components) to use TypeScript interfaces
- Changed documentation structure to use `typedoc` available as a part of redistributables or online at https://kaspa.aspectron.org/docs/
- Project-wide documentation updates
- Additional self-contained Web Browser examples
- Modified the structure of WASM32 SDK release to include all variants of libraries (both release and dev builds), examples and documentation in a single package.
