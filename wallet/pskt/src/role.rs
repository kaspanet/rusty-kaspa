//! PSKT roles.

/// Initializes the PSKT with 0 inputs and 0 outputs.
/// Reference: [BIP-370: Creator](https://github.com/bitcoin/bips/blob/master/bip-0370.mediawiki#creator)
pub enum Creator {}

/// Adds inputs and outputs to the PSKT.
/// Reference: [BIP-370: Constructor](https://github.com/bitcoin/bips/blob/master/bip-0370.mediawiki#constructor)
pub enum Constructor {}

/// Can set the sequence number.
/// Reference: [BIP-370: Updater](https://github.com/bitcoin/bips/blob/master/bip-0370.mediawiki#updater)
pub enum Updater {}

/// Creates cryptographic signatures for the inputs using private keys.
/// Reference: [BIP-370: Signer](https://github.com/bitcoin/bips/blob/master/bip-0370.mediawiki#signer)
pub enum Signer {}

/// Merges multiple PSKTs into one.
/// Reference: [BIP-174: Combiner](https://github.com/bitcoin/bips/blob/master/bip-0174.mediawiki#combiner)
pub enum Combiner {}

/// Completes the PSKT, ensuring all inputs have valid signatures, and finalizes the transaction.
/// Reference: [BIP-174: Input Finalizer](https://github.com/bitcoin/bips/blob/master/bip-0174.mediawiki#input-finalizer)
pub enum Finalizer {}

/// Extracts the final transaction from the PSKT once all parts are in place and the PSKT is fully signed.
/// Reference: [BIP-370: Transaction Extractor](https://github.com/bitcoin/bips/blob/master/bip-0370.mediawiki#transaction-extractor)
pub enum Extractor {}
