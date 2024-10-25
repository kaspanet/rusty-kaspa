use crate::error::Error;
use crate::result::Result;
use kaspa_consensus_core::constants::SOMPI_PER_KASPA;
use std::fmt::Display;

pub fn try_parse_required_nonzero_kaspa_as_sompi_u64<S: ToString + Display>(kaspa_amount: Option<S>) -> Result<u64> {
    if let Some(kaspa_amount) = kaspa_amount {
        let sompi_amount = kaspa_amount
            .to_string()
            .parse::<f64>()
            .map_err(|_| Error::custom(format!("Supplied Kaspa amount is not valid: '{kaspa_amount}'")))?
            * SOMPI_PER_KASPA as f64;
        if sompi_amount < 0.0 {
            Err(Error::custom("Supplied Kaspa amount is not valid: '{kaspa_amount}'"))
        } else {
            let sompi_amount = sompi_amount as u64;
            if sompi_amount == 0 {
                Err(Error::custom("Supplied required kaspa amount must not be a zero: '{kaspa_amount}'"))
            } else {
                Ok(sompi_amount)
            }
        }
    } else {
        Err(Error::custom("Missing Kaspa amount"))
    }
}

pub fn try_parse_required_kaspa_as_sompi_u64<S: ToString + Display>(kaspa_amount: Option<S>) -> Result<u64> {
    if let Some(kaspa_amount) = kaspa_amount {
        let sompi_amount = kaspa_amount
            .to_string()
            .parse::<f64>()
            .map_err(|_| Error::custom(format!("Supplied Kasapa amount is not valid: '{kaspa_amount}'")))?
            * SOMPI_PER_KASPA as f64;
        if sompi_amount < 0.0 {
            Err(Error::custom("Supplied Kaspa amount is not valid: '{kaspa_amount}'"))
        } else {
            Ok(sompi_amount as u64)
        }
    } else {
        Err(Error::custom("Missing Kaspa amount"))
    }
}

pub fn try_parse_optional_kaspa_as_sompi_i64<S: ToString + Display>(kaspa_amount: Option<S>) -> Result<Option<i64>> {
    if let Some(kaspa_amount) = kaspa_amount {
        let sompi_amount = kaspa_amount
            .to_string()
            .parse::<f64>()
            .map_err(|_e| Error::custom(format!("Supplied Kasapa amount is not valid: '{kaspa_amount}'")))?
            * SOMPI_PER_KASPA as f64;
        if sompi_amount < 0.0 {
            Err(Error::custom("Supplied Kaspa amount is not valid: '{kaspa_amount}'"))
        } else {
            Ok(Some(sompi_amount as i64))
        }
    } else {
        Ok(None)
    }
}
