use crate::imports::*;
use crate::result::Result;

/// Maturity period for coinbase transactions.
pub static UTXO_MATURITY_PERIOD_COINBASE_TRANSACTION_DAA: AtomicU64 = AtomicU64::new(128);
/// Maturity period for user transactions.
pub static UTXO_MATURITY_PERIOD_USER_TRANSACTION_DAA: AtomicU64 = AtomicU64::new(16);
/// Recovery period for UTXOs used in transactions.
pub static UTXO_RECOVERY_PERIOD_SECONDS: AtomicU64 = AtomicU64::new(180);

#[derive(Default)]
pub struct UtxoProcessingSettings {
    pub coinbase_transaction_maturity_daa: Option<u64>,
    pub user_transaction_maturity_daa: Option<u64>,
    pub utxo_recovery_period_seconds: Option<u64>,
}

impl UtxoProcessingSettings {
    pub fn new(
        coinbase_transaction_maturity_daa: Option<u64>,
        user_transaction_maturity_daa: Option<u64>,
        utxo_recovery_period_seconds: Option<u64>,
    ) -> Self {
        Self { coinbase_transaction_maturity_daa, user_transaction_maturity_daa, utxo_recovery_period_seconds }
    }

    pub fn init(thresholds: UtxoProcessingSettings) {
        if let Some(v) = thresholds.coinbase_transaction_maturity_daa {
            UTXO_MATURITY_PERIOD_COINBASE_TRANSACTION_DAA.store(v, Ordering::Relaxed)
        }
        if let Some(v) = thresholds.user_transaction_maturity_daa {
            UTXO_MATURITY_PERIOD_USER_TRANSACTION_DAA.store(v, Ordering::Relaxed)
        }
        if let Some(v) = thresholds.utxo_recovery_period_seconds {
            UTXO_RECOVERY_PERIOD_SECONDS.store(v, Ordering::Relaxed)
        }
    }
}

#[wasm_bindgen(js_name = configureUtxoProcessing)]
pub fn configure_utxo_processing(thresholds: &JsValue) -> Result<()> {
    let object = Object::try_from(thresholds).ok_or(Error::custom("Supplied value must be an object"))?;
    let coinbase_transaction_maturity_daa = object.get_u64("coinbaseTransactionMaturityInDAA").ok();
    let user_transaction_maturity_daa = object.get_u64("userTransactionMaturityInDAA").ok();
    let utxo_recovery_period_seconds = object.get_u64("utxoRecoveryPeriodInSeconds").ok();

    let thresholds =
        UtxoProcessingSettings { coinbase_transaction_maturity_daa, user_transaction_maturity_daa, utxo_recovery_period_seconds };

    UtxoProcessingSettings::init(thresholds);

    Ok(())
}
