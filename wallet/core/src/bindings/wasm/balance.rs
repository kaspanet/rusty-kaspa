use crate::imports::*;
use crate::result::Result;
use crate::utxo::balance as native;
use kaspa_consensus_core::network::NetworkTypeT;

///
/// Represents a {@link UtxoContext} (account) balance.
///
/// @see {@link IBalance}, {@link UtxoContext}
///
/// @category Wallet SDK
///
#[wasm_bindgen]
pub struct Balance {
    inner: native::Balance,
}

#[wasm_bindgen]
impl Balance {
    /// Confirmed amount of funds available for spending.
    #[wasm_bindgen(getter)]
    pub fn mature(&self) -> BigInt {
        self.inner.mature.into()
    }

    /// Amount of funds that are being received and are not yet confirmed.
    #[wasm_bindgen(getter)]
    pub fn pending(&self) -> BigInt {
        self.inner.pending.into()
    }

    /// Amount of funds that are being send and are not yet accepted by the network.
    #[wasm_bindgen(getter)]
    pub fn outgoing(&self) -> BigInt {
        self.inner.outgoing.into()
    }

    #[wasm_bindgen(js_name = "toBalanceStrings")]
    pub fn to_balance_strings(&self, network_type: &NetworkTypeT) -> Result<BalanceStrings> {
        let network_type = NetworkType::try_from(network_type)?;
        Ok(native::BalanceStrings::from((Some(&self.inner), &network_type, None)).into())
    }
}

impl From<native::Balance> for Balance {
    fn from(inner: native::Balance) -> Self {
        Self { inner }
    }
}

///
/// Formatted string representation of the {@link Balance}.
///
/// The value is formatted as `123,456.789`.
///
/// @category Wallet SDK
///
#[wasm_bindgen]
pub struct BalanceStrings {
    inner: native::BalanceStrings,
}

#[wasm_bindgen]
impl BalanceStrings {
    #[wasm_bindgen(getter)]
    pub fn mature(&self) -> String {
        self.inner.mature.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn pending(&self) -> Option<String> {
        self.inner.pending.clone()
    }
}

impl From<native::BalanceStrings> for BalanceStrings {
    fn from(inner: native::BalanceStrings) -> Self {
        Self { inner }
    }
}
