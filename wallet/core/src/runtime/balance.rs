use crate::imports::*;
use kaspa_consensus_core::network::NetworkType;

pub enum DeltaStyle {
    Mature,
    Pending,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
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

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    pub mature: u64,
    pub pending: u64,
    mature_delta: Delta,
    pending_delta: Delta,
}

impl Balance {
    pub fn new(mature: u64, pending: u64) -> Self {
        Self { mature, pending, mature_delta: Delta::default(), pending_delta: Delta::default() }
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
}

#[derive(Default, Debug)]
pub struct AtomicBalance {
    pub mature: AtomicU64,
    pub pending: AtomicU64,
}

impl AtomicBalance {
    pub fn add(&self, balance: Balance) {
        self.mature.fetch_add(balance.mature, Ordering::SeqCst);
        self.pending.fetch_add(balance.pending, Ordering::SeqCst);
    }
}

impl From<AtomicBalance> for Balance {
    fn from(atomic_balance: AtomicBalance) -> Self {
        Self {
            mature: atomic_balance.mature.load(Ordering::SeqCst),
            pending: atomic_balance.pending.load(Ordering::SeqCst),
            mature_delta: Delta::default(),
            pending_delta: Delta::default(),
        }
    }
}

pub struct BalanceStrings {
    pub mature: String,
    pub pending: Option<String>,
}

impl From<(&Option<Balance>, &NetworkType, Option<usize>)> for BalanceStrings {
    fn from((balance, network_type, padding): (&Option<Balance>, &NetworkType, Option<usize>)) -> Self {
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
