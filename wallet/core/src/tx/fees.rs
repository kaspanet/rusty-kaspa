use crate::result::Result;
use wasm_bindgen::prelude::*;
use workflow_wasm::prelude::*;

/// Transaction fees.  Fees are comprised of 2 values:
///
/// `relay` fees - mandatory fees that are required to relay the transaction
/// `priority` fees - optional fees applied to the final outgoing transaction
/// in addition to `relay` fees.
///
/// Fees can be:
/// - `SenderPaysAll` - (standard) fees are added to outgoing transaction value
/// - `ReceiverPaysTransfer` - aggregation fees are paid by sender, but final
/// transaction fees, including priority fees are paid by the receiver.
/// - `ReceiverPaysAll` - all transaction fees are paid by the receiver.
///
/// NOTE: If priority fees are `0`, fee variants can be used control
/// who pays the `relay` fees.
///
/// NOTE: `ReceiverPays` variants can fail during the aggregation process
/// if there are not enough funds to cover the final transaction.
/// There are 2 solutions to this problem:
///
/// 1. Use estimation to check that the funds are sufficient.
/// 2. Check balance and ensure that there is a sufficient amount of funds.
///
#[derive(Debug, Clone)]
pub enum Fees {
    /// Fee management disabled (sweep transactions, pays all fees)
    None,
    /// fees are are added to the transaction value
    SenderPaysAll(u64),
    /// all transaction fees are subtracted from transaction value
    ReceiverPaysAll(u64),
    /// final transaction fees are subtracted from transaction value
    ReceiverPaysTransfer(u64),
}

impl Fees {
    pub fn is_none(&self) -> bool {
        matches!(self, Fees::None)
    }
}

/// This trait converts supplied positive `i64` value as `Exclude` fees
/// and negative `i64` value as `Include` fees. I.e. `Fees::from(-100)` will
/// result in priority fees that are included in the transaction value.
impl From<i64> for Fees {
    fn from(fee: i64) -> Self {
        if fee < 0 {
            Fees::ReceiverPaysTransfer(fee.unsigned_abs())
        } else {
            Fees::SenderPaysAll(fee as u64)
        }
    }
}

impl From<u64> for Fees {
    fn from(fee: u64) -> Self {
        Fees::SenderPaysAll(fee)
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

impl TryFrom<JsValue> for Fees {
    type Error = crate::error::Error;
    fn try_from(fee: JsValue) -> Result<Self> {
        if fee.is_undefined() || fee.is_null() {
            Ok(Fees::None)
        } else if let Ok(fee) = fee.try_as_u64() {
            Ok(Fees::SenderPaysAll(fee))
        } else {
            Err(crate::error::Error::custom("Invalid fee"))
        }
    }
}
