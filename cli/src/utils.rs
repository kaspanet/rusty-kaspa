use crate::error::Error;
use crate::result::Result;
use aero_consensus_core::constants::SOMPI_PER_AERO;
use std::fmt::Display;

pub fn try_parse_required_nonzero_aero_as_sompi_u64<S: ToString + Display>(aero_amount: Option<S>) -> Result<u64> {
    if let Some(aero_amount) = aero_amount {
        let sompi_amount = aero_amount
            .to_string()
            .parse::<f64>()
            .map_err(|_| Error::custom(format!("Supplied Aero amount is not valid: '{aero_amount}'")))?
            * SOMPI_PER_AERO as f64;
        if sompi_amount < 0.0 {
            Err(Error::custom("Supplied Aero amount is not valid: '{aero_amount}'"))
        } else {
            let sompi_amount = sompi_amount as u64;
            if sompi_amount == 0 {
                Err(Error::custom("Supplied required aero amount must not be a zero: '{aero_amount}'"))
            } else {
                Ok(sompi_amount)
            }
        }
    } else {
        Err(Error::custom("Missing Aero amount"))
    }
}

pub fn try_parse_required_aero_as_sompi_u64<S: ToString + Display>(aero_amount: Option<S>) -> Result<u64> {
    if let Some(aero_amount) = aero_amount {
        let sompi_amount = aero_amount
            .to_string()
            .parse::<f64>()
            .map_err(|_| Error::custom(format!("Supplied Kasapa amount is not valid: '{aero_amount}'")))?
            * SOMPI_PER_AERO as f64;
        if sompi_amount < 0.0 {
            Err(Error::custom("Supplied Aero amount is not valid: '{aero_amount}'"))
        } else {
            Ok(sompi_amount as u64)
        }
    } else {
        Err(Error::custom("Missing Aero amount"))
    }
}

pub fn try_parse_optional_aero_as_sompi_i64<S: ToString + Display>(aero_amount: Option<S>) -> Result<Option<i64>> {
    if let Some(aero_amount) = aero_amount {
        let sompi_amount = aero_amount
            .to_string()
            .parse::<f64>()
            .map_err(|_e| Error::custom(format!("Supplied Kasapa amount is not valid: '{aero_amount}'")))?
            * SOMPI_PER_AERO as f64;
        if sompi_amount < 0.0 {
            Err(Error::custom("Supplied Aero amount is not valid: '{aero_amount}'"))
        } else {
            Ok(Some(sompi_amount as i64))
        }
    } else {
        Ok(None)
    }
}
