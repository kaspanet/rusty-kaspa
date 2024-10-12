use std::fmt::{Display, Formatter};

use kaspa_addresses::Address;

#[derive(Debug, Clone)]
pub enum ReturnAddress {
    Found(Address),
    AlreadyPruned,
    TxFromCoinbase,
    NoTxAtScore,
    NonStandard,
    NotFound(String),
}

impl Display for ReturnAddress {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ReturnAddress::AlreadyPruned => "Transaction is already pruned".to_string(),
            ReturnAddress::NoTxAtScore => "Transaction not found at given accepting daa score".to_string(),
            ReturnAddress::NonStandard => "Transaction was found but not standard".to_string(),
            ReturnAddress::TxFromCoinbase => "Transaction return address is coinbase".to_string(),
            ReturnAddress::NotFound(reason) => format!("Transaction return address not found: {}", reason),
            ReturnAddress::Found(address) => address.to_string(),
        };
        f.write_str(&s)
    }
}
