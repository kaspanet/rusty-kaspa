//!
//! Wallet transaction record types.
//!

use crate::imports::*;
pub use kaspa_consensus_core::tx::TransactionId;

#[wasm_bindgen(typescript_custom_section)]
const TS_TRANSACTION_KIND: &'static str = r#"
/**
 * 
 * 
 * @category Wallet SDK
 * 
 */
export enum TransactionKind {
    Reorg = "reorg",
    Stasis = "stasis",
    Batch = "batch",
    Change = "change",
    Incoming = "incoming",
    Outgoing = "outgoing",
    External = "external",
    TransferIncoming = "transfer-incoming",
    TransferOutgoing = "transfer-outgoing",
}
"#;

// Do not change the order of the variants in this enum.
seal! { 0x93c6, {
        #[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize, Eq, PartialEq)]
        #[serde(rename_all = "kebab-case")]
        pub enum TransactionKind {
            /// Reorg transaction (caused by UTXO reorg).
            /// NOTE: These transactions should be ignored by clients
            /// if the transaction has not reached Pending maturity.
            Reorg,
            /// Stasis transaction (caused by a reorg during coinbase UTXO stasis).
            /// NOTE: These types of transactions should be ignored by clients.
            Stasis,
            /// Internal batch (sweep) transaction. Generated as a part
            /// of Outgoing or Transfer transactions if the number of
            /// UTXOs needed for transaction is greater than the transaction
            /// mass limit.
            Batch,
            /// Change transaction. Generated as a part of the Outgoing
            /// or Transfer transactions.
            /// NOTE: These types of transactions should be ignored by clients
            Change,
            /// A regular incoming transaction comprised of one or more UTXOs.
            Incoming,
            /// An outgoing transaction created by the wallet framework.
            /// If transaction creation results in multiple sweep transactions,
            /// this is the final transaction in the transaction tree.
            Outgoing,
            /// Externally triggered *Outgoing* transaction observed by
            /// the wallet runtime. This only occurs when another wallet
            /// issues an outgoing transaction from addresses monitored
            /// by this instance of the wallet (for example a copy of
            /// the wallet or an account).
            External,
            /// Incoming transfer transaction. A transfer between multiple
            /// accounts managed by the wallet runtime.
            TransferIncoming,
            /// Outgoing transfer transaction. A transfer between multiple
            /// accounts managed by the wallet runtime.
            TransferOutgoing,
        }
    }
}

impl TransactionKind {}

impl TransactionKind {
    pub fn sign(&self) -> String {
        match self {
            TransactionKind::Incoming => "+",
            TransactionKind::Outgoing => "-",
            TransactionKind::External => "-",
            TransactionKind::Batch => "",
            TransactionKind::Reorg => "-",
            TransactionKind::Stasis => "",
            TransactionKind::TransferIncoming => "",
            TransactionKind::TransferOutgoing => "",
            TransactionKind::Change => "",
        }
        .to_string()
    }
}

impl std::fmt::Display for TransactionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            TransactionKind::Incoming => "incoming",
            TransactionKind::Outgoing => "outgoing",
            TransactionKind::External => "external",
            TransactionKind::Batch => "batch",
            TransactionKind::Reorg => "reorg",
            TransactionKind::Stasis => "stasis",
            TransactionKind::TransferIncoming => "transfer-incoming",
            TransactionKind::TransferOutgoing => "transfer-outgoing",
            TransactionKind::Change => "change",
        };
        write!(f, "{s}")
    }
}

impl TryFrom<JsValue> for TransactionKind {
    type Error = Error;
    fn try_from(js_value: JsValue) -> std::result::Result<Self, Self::Error> {
        if let Some(s) = js_value.as_string() {
            match s.as_str() {
                "incoming" => Ok(TransactionKind::Incoming),
                "outgoing" => Ok(TransactionKind::Outgoing),
                "external" => Ok(TransactionKind::External),
                "batch" => Ok(TransactionKind::Batch),
                "reorg" => Ok(TransactionKind::Reorg),
                "stasis" => Ok(TransactionKind::Stasis),
                "transfer-incoming" => Ok(TransactionKind::TransferIncoming),
                "transfer-outgoing" => Ok(TransactionKind::TransferOutgoing),
                "change" => Ok(TransactionKind::Change),
                _ => Err(Error::InvalidTransactionKind(s)),
            }
        } else {
            Err(Error::InvalidTransactionKind(format!("{:?}", js_value)))
        }
    }
}
