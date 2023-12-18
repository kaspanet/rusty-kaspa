//!
//! Wallet framework settings that control maturity
//! durations.
//!

use crate::imports::*;
use crate::result::Result;

/// Maturity period for coinbase transactions.
pub static UTXO_MATURITY_PERIOD_COINBASE_TRANSACTION_DAA: AtomicU64 = AtomicU64::new(100);
/// Stasis period for coinbase transactions (no standard notifications occur until the
/// coinbase tx is out of stasis).
pub static UTXO_STASIS_PERIOD_COINBASE_TRANSACTION_DAA: AtomicU64 = AtomicU64::new(50);
/// Maturity period for user transactions.
pub static UTXO_MATURITY_PERIOD_USER_TRANSACTION_DAA: AtomicU64 = AtomicU64::new(10);
/// Enables wallet events containing context UTXO updates.
/// Useful if the client wants to keep track of UTXO sets or
/// supply them during creation of transactions.
pub static ENABLE_UTXO_SELECTION_EVENTS: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
pub struct UtxoProcessingSettings {
    pub coinbase_transaction_maturity_daa: Option<u64>,
    pub user_transaction_maturity_daa: Option<u64>,
    pub enable_utxo_selection_events: Option<bool>,
}

impl UtxoProcessingSettings {
    pub fn new(
        coinbase_transaction_maturity_daa: Option<u64>,
        user_transaction_maturity_daa: Option<u64>,
        enable_utxo_selection_events: Option<bool>,
    ) -> Self {
        Self { coinbase_transaction_maturity_daa, user_transaction_maturity_daa, enable_utxo_selection_events }
    }

    pub fn init(settings: UtxoProcessingSettings) {
        if let Some(v) = settings.coinbase_transaction_maturity_daa {
            UTXO_MATURITY_PERIOD_COINBASE_TRANSACTION_DAA.store(v, Ordering::Relaxed)
        }
        if let Some(v) = settings.user_transaction_maturity_daa {
            UTXO_MATURITY_PERIOD_USER_TRANSACTION_DAA.store(v, Ordering::Relaxed)
        }
        if let Some(v) = settings.enable_utxo_selection_events {
            ENABLE_UTXO_SELECTION_EVENTS.store(v, Ordering::Relaxed)
        }
    }
}

#[wasm_bindgen(js_name = configureUtxoProcessing)]
pub fn configure_utxo_processing(thresholds: &JsValue) -> Result<()> {
    let object = Object::try_from(thresholds).ok_or(Error::custom("Supplied value must be an object"))?;
    let coinbase_transaction_maturity_daa = object.get_u64("coinbaseTransactionMaturityInDAA").ok();
    let user_transaction_maturity_daa = object.get_u64("userTransactionMaturityInDAA").ok();
    let enable_utxo_selection_events = object.get_bool("enableUtxoSelectionEvents").ok();

    let thresholds =
        UtxoProcessingSettings { coinbase_transaction_maturity_daa, user_transaction_maturity_daa, enable_utxo_selection_events };

    UtxoProcessingSettings::init(thresholds);

    Ok(())
}
