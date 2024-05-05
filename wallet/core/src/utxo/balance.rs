//!
//! Account balances.
//!

use crate::imports::*;

pub enum DeltaStyle {
    Mature,
    Pending,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum Delta {
    #[default]
    NoChange = 0,
    Increase,
    Decrease,
}

impl Delta {
    pub fn style(&self, s: &str, delta_style: DeltaStyle) -> String {
        match self {
            Delta::NoChange => "".to_string() + s,
            Delta::Increase => style(s).green().to_string(),
            Delta::Decrease => {
                if matches!(delta_style, DeltaStyle::Mature) {
                    style(s).red().to_string()
                } else {
                    style(s).dim().to_string()
                }
            }
        }
    }
}

impl From<std::cmp::Ordering> for Delta {
    fn from(o: std::cmp::Ordering) -> Self {
        match o {
            std::cmp::Ordering::Less => Delta::Decrease,
            std::cmp::Ordering::Greater => Delta::Increase,
            std::cmp::Ordering::Equal => Delta::NoChange,
        }
    }
}

#[wasm_bindgen(typescript_custom_section)]
const TS_BALANCE: &'static str = r#"
/**
 * {@link UtxoContext} (wallet account) balance.
 * @category Wallet SDK
 */
export interface IBalance {
    /**
     * Total amount of Kaspa (in SOMPI) available for 
     * spending.
     */
    mature: bigint;
    /**
     * Total amount of Kaspa (in SOMPI) that has been 
     * received and is pending confirmation.
     */
    pending: bigint;
    /**
     * Total amount of Kaspa (in SOMPI) currently 
     * being sent as a part of the outgoing transaction
     * but has not yet been accepted by the network.
     */
    outgoing: bigint;
    /**
     * Number of UTXOs available for spending.
     */
    matureUtxoCount: number;
    /**
     * Number of UTXOs that have been received and 
     * are pending confirmation.
     */
    pendingUtxoCount: number;
    /**
     * Number of UTXOs currently in stasis (coinbase 
     * transactions received as a result of mining).
     * Unlike regular user transactions, coinbase 
     * transactions go through `stasis->pending->mature`
     * stages. Client applications should ignore `stasis`
     * stages and should process transactions only when
     * they have reached the `pending` stage. However, 
     * `stasis` information can be used for informative 
     * purposes to indicate that coinbase transactions
     * have arrived.
     */
    stasisUtxoCount: number;
}
"#;

#[derive(Default, Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct Balance {
    pub mature: u64,
    pub pending: u64,
    pub outgoing: u64,
    pub mature_utxo_count: usize,
    pub pending_utxo_count: usize,
    pub stasis_utxo_count: usize,
    #[serde(skip)]
    mature_delta: Delta,
    #[serde(skip)]
    pending_delta: Delta,
}

impl Balance {
    pub fn new(
        mature: u64,
        pending: u64,
        outgoing: u64,
        mature_utxo_count: usize,
        pending_utxo_count: usize,
        stasis_utxo_count: usize,
    ) -> Self {
        Self {
            mature,
            pending,
            outgoing,
            mature_delta: Delta::default(),
            pending_delta: Delta::default(),
            mature_utxo_count,
            pending_utxo_count,
            stasis_utxo_count,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.mature == 0 && self.pending == 0
    }

    pub fn delta(&mut self, previous: &Option<Balance>) {
        if let Some(previous) = previous {
            self.mature_delta = self.mature.cmp(&previous.mature).into();
            self.pending_delta = self.pending.cmp(&previous.pending).into();
        } else {
            self.mature_delta = Delta::NoChange;
            self.pending_delta = Delta::NoChange;
        }
    }

    pub fn to_balance_strings(&self, network_type: &NetworkType, padding: Option<usize>) -> BalanceStrings {
        (Some(self), network_type, padding).into()
    }
}

#[derive(Default, Debug)]
pub struct AtomicBalance {
    pub mature: AtomicU64,
    pub pending: AtomicU64,
    pub mature_utxos: AtomicUsize,
    pub pending_utxos: AtomicUsize,
    pub stasis_utxos: AtomicUsize,
}

impl AtomicBalance {
    pub fn add(&self, balance: Balance) {
        self.mature.fetch_add(balance.mature, Ordering::SeqCst);
        self.pending.fetch_add(balance.pending, Ordering::SeqCst);
        self.mature_utxos.fetch_add(balance.mature_utxo_count, Ordering::SeqCst);
        self.pending_utxos.fetch_add(balance.pending_utxo_count, Ordering::SeqCst);
        self.stasis_utxos.fetch_add(balance.stasis_utxo_count, Ordering::SeqCst);
    }
}

impl From<AtomicBalance> for Balance {
    fn from(atomic_balance: AtomicBalance) -> Self {
        Self {
            mature: atomic_balance.mature.load(Ordering::SeqCst),
            pending: atomic_balance.pending.load(Ordering::SeqCst),
            outgoing: 0,
            mature_utxo_count: atomic_balance.mature_utxos.load(Ordering::SeqCst),
            pending_utxo_count: atomic_balance.pending_utxos.load(Ordering::SeqCst),
            stasis_utxo_count: atomic_balance.stasis_utxos.load(Ordering::SeqCst),
            mature_delta: Delta::default(),
            pending_delta: Delta::default(),
        }
    }
}

pub struct BalanceStrings {
    pub mature: String,
    pub pending: Option<String>,
}

impl From<(Option<&Balance>, &NetworkType, Option<usize>)> for BalanceStrings {
    fn from((balance, network_type, padding): (Option<&Balance>, &NetworkType, Option<usize>)) -> Self {
        let suffix = utils::kaspa_suffix(network_type);
        if let Some(balance) = balance {
            let mut mature = utils::sompi_to_kaspa_string(balance.mature);
            let mut pending = if balance.pending > 0 { Some(utils::sompi_to_kaspa_string(balance.pending)) } else { None };
            if let Some(padding) = padding {
                mature = mature.pad_to_width(padding);
                pending = pending.map(|pending| pending.pad_to_width(padding));
            }
            Self {
                mature: format!("{} {}", balance.mature_delta.style(&mature, DeltaStyle::Mature), suffix),
                pending: pending.map(|pending| format!("{} {}", balance.pending_delta.style(&pending, DeltaStyle::Pending), suffix)),
            }
        } else {
            Self { mature: format!("N/A {suffix}"), pending: None }
        }
    }
}

impl std::fmt::Display for BalanceStrings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(pending) = &self.pending {
            write!(f, "{} ({} pending)", self.mature, pending)
        } else {
            write!(f, "{}", self.mature)
        }
    }
}
