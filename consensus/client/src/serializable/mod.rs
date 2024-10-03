//!
//! # Standardized JSON serialization and deserialization of Kaspa transactions.
//!
//! This module provides standardized JSON serialization and deserialization of
//! Kaspa transactions. There are two sub-modules: `numeric` and `string`.
//!
//! The `numeric` module provides serialization and deserialization of transactions
//! with all large integer values as `bigint` types in WASM or numerical values that
//! exceed the largest integer that can be represented by the JavaScript `number` type.
//!
//! The `string` module provides serialization and deserialization of transactions
//! with all large integer values as `string` types. This allows deserialization
//! via JSON in JavaScript environments and later conversion to `bigint` types.
//!
//! These data structures can be used for manual transport of transactions using JSON.
//! For more advanced use cases, please refer to `PSKT` in the [`kaspa_wallet_pskt`](https://docs.rs/kaspa_wallet_pskt)
//! crate.
//!

#![allow(non_snake_case)]

pub mod numeric;
pub mod string;

use wasm_bindgen::prelude::*;
#[wasm_bindgen(typescript_custom_section)]
const TS_TYPES: &'static str = r#"

/**
 * Interface defines the structure of a serializable UTXO entry.
 * 
 * @see {@link ISerializableTransactionInput}, {@link ISerializableTransaction}
 * @category Wallet SDK
 */
export interface ISerializableUtxoEntry {
    address?: Address;
    amount: bigint;
    scriptPublicKey: ScriptPublicKey;
    blockDaaScore: bigint;
    isCoinbase: boolean;
}

/**
 * Interface defines the structure of a serializable transaction input.
 * 
 * @see {@link ISerializableTransaction}
 * @category Wallet SDK
 */
export interface ISerializableTransactionInput {
    transactionId : HexString;
    index: number;
    sequence: bigint;
    sigOpCount: number;
    signatureScript?: HexString;
    utxo: ISerializableUtxoEntry;
}

/**
 * Interface defines the structure of a serializable transaction output.
 * 
 * @see {@link ISerializableTransaction}
 * @category Wallet SDK
 */
export interface ISerializableTransactionOutput {
    value: bigint;
    scriptPublicKey: IScriptPublicKey;
}

/**
 * Interface defines the structure of a serializable transaction.
 * 
 * Serializable transactions can be produced using 
 * {@link Transaction.serializeToJSON},
 * {@link Transaction.serializeToSafeJSON} and 
 * {@link Transaction.serializeToObject} 
 * functions for processing (signing) in external systems.
 * 
 * Once the transaction is signed, it can be deserialized
 * into {@link Transaction} using {@link Transaction.deserializeFromJSON}
 * and {@link Transaction.deserializeFromSafeJSON} functions. 
 * 
 * @see {@link Transaction},
 * {@link ISerializableTransactionInput},
 * {@link ISerializableTransactionOutput},
 * {@link ISerializableUtxoEntry}
 * 
 * @category Wallet SDK
 */
export interface ISerializableTransaction {
    id? : HexString;
    version: number;
    inputs: ISerializableTransactionInput[];
    outputs: ISerializableTransactionOutput[];
    lockTime: bigint;
    subnetworkId: HexString;
    gas: bigint;
    payload: HexString;
}

"#;

#[wasm_bindgen]
extern "C" {
    /// WASM (TypeScript) representation of the `ISerializableTransaction` interface.
    #[wasm_bindgen(extends = js_sys::Array, typescript_type = "ISerializableTransaction")]
    pub type SerializableTransactionT;
}
