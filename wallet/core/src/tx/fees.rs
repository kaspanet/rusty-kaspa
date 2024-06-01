//!
//! Primitives for declaring transaction fees.
//!

use crate::result::Result;
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// Transaction fees.  Fees are comprised of 2 values:
///
/// `relay` fees - mandatory fees that are required to relay the transaction
/// `priority` fees - optional fees applied to the final outgoing transaction
/// in addition to `relay` fees.
///
/// Fees can be:
/// - `SenderPaysAll` - (standard) fees are added to outgoing transaction value
/// - `ReceiverPaysAll` - all transaction fees are paid by the receiver.
///
/// NOTE: If priority fees are `0`, fee variants can be used control
/// who pays the `network` fees.
///
/// NOTE: `ReceiverPays` variants can fail during the aggregation process
/// if there are not enough funds to cover the final transaction.
/// There are 2 solutions to this problem:
///
/// 1. Use estimation to check that the funds are sufficient.
/// 2. Check balance and ensure that there is a sufficient amount of funds.
///
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum Fees {
    /// Fee management disabled (sweep transactions, pays all fees)
    None,
    /// all fees are added to the final transaction value
    SenderPays(u64),
    /// all fees are subtracted from the final transaction value
    ReceiverPays(u64),
}

impl Fees {
    pub fn is_none(&self) -> bool {
        matches!(self, Fees::None)
    }

    pub fn sender_pays(&self) -> bool {
        matches!(self, Fees::SenderPays(_))
    }

    pub fn receiver_pays(&self) -> bool {
        matches!(self, Fees::ReceiverPays(_))
    }

    pub fn additional(&self) -> u64 {
        match self {
            Fees::SenderPays(fee) => *fee,
            _ => 0,
        }
    }
}

/// This trait converts supplied positive `i64` value as `Exclude` fees
/// and negative `i64` value as `Include` fees. I.e. `Fees::from(-100)` will
/// result in priority fees that are included in the transaction value.
impl From<i64> for Fees {
    fn from(fee: i64) -> Self {
        if fee < 0 {
            Fees::ReceiverPays(fee.unsigned_abs())
        } else {
            Fees::SenderPays(fee as u64)
        }
    }
}

impl From<u64> for Fees {
    fn from(fee: u64) -> Self {
        Fees::SenderPays(fee)
    }
}

impl TryFrom<&str> for Fees {
    type Error = crate::error::Error;
    fn try_from(fee: &str) -> Result<Self> {
        if fee.is_empty() {
            Ok(Fees::None)
        } else {
            let fee = crate::utils::try_kaspa_str_to_sompi_i64(fee)?.unwrap_or(0);
            Ok(Fees::from(fee))
        }
    }
}

impl TryFrom<String> for Fees {
    type Error = crate::error::Error;
    fn try_from(fee: String) -> Result<Self> {
        Self::try_from(fee.as_str())
    }
}
