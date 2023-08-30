use crate::opcodes::codes::{OpCheckMultiSig, OpCheckMultiSigECDSA};
use crate::script_builder::{ScriptBuilder, ScriptBuilderError};
use kaspa_addresses::{Address, Version};
use thiserror::Error;

#[derive(Error, PartialEq, Eq, Debug, Clone)]
pub enum Error {
    // ErrTooManyRequiredSigs is returned from multisig_script when the
    // specified number of required signatures is larger than the number of
    // provided public keys.
    #[error("too many required signatures")]
    ErrTooManyRequiredSigs,
    #[error(transparent)]
    ScriptBuilderError(#[from] ScriptBuilderError),
    #[error("public key address version should be the same for all provided keys")]
    WrongVersion,
    #[error("provided public keys should not be empty")]
    EmptyKeys,
}

/// Generates a multi-signature redeem script from sorted public keys.
///
/// This function builds a redeem script requiring `required` out of the
/// already sorted `pub_keys` given. It is expected that the public keys
/// are provided in a sorted order.
///
/// # Parameters
///
/// * `pub_keys`: An iterator over sorted public key addresses.
/// * `required`: The number of required signatures to spend the funds.
///
/// # Returns
///
/// A `Result` containing the redeem script in the form of a `Vec<u8>`
/// or an error of type `Error`.
///
/// # Errors
///
/// This function will return an error if:
/// * The number of provided keys is less than `required`.
/// * The public keys contain an unexpected version.
/// * There are no public keys provided.
pub fn multisig_redeem_script_sorted<'a>(mut pub_keys: impl Iterator<Item = &'a Address>, required: usize) -> Result<Vec<u8>, Error> {
    if pub_keys.size_hint().1.is_some_and(|upper| upper < required) {
        return Err(Error::ErrTooManyRequiredSigs);
    };
    let mut builder = ScriptBuilder::new();
    builder.add_i64(required as i64)?;

    let mut count = 0i64;
    let mut version: Version;
    match pub_keys.next() {
        None => return Err(Error::EmptyKeys),
        Some(pub_key) => {
            count += 1;
            builder.add_data(pub_key.payload.as_slice())?;
            version = match pub_key.version {
                pk @ Version::PubKey => pk,
                pk @ Version::PubKeyECDSA => pk,
                Version::ScriptHash => {
                    return Err(Error::WrongVersion); // todo is it correct?
                }
            }
        }
    };

    for pub_key in pub_keys {
        count += 1;
        builder.add_data(pub_key.payload.as_slice())?;
    }
    if (count as usize) < required {
        return Err(Error::ErrTooManyRequiredSigs);
    }

    builder.add_i64(count)?;
    if version == Version::PubKeyECDSA {
        builder.add_op(OpCheckMultiSigECDSA)?;
    } else {
        builder.add_op(OpCheckMultiSig)?;
    }

    Ok(builder.drain())
}

///
/// This function sorts the provided public keys and then constructs
/// a redeem script requiring `required` out of the sorted keys.
///
/// # Parameters
///
/// * `pub_keys`: A mutable slice of public key addresses. The keys can be in any order.
/// * `required`: The number of required signatures to spend the funds.
///
/// # Returns
///
/// A `Result` containing the redeem script in the form of a `Vec<u8>`
/// or an error of type `Error`.
///
/// # Errors
///
/// This function will return an error if:
/// * The number of provided keys is less than `required`.
/// * The public keys contain an unexpected version.
/// * There are no public keys provided.
pub fn multisig_redeem_script(pub_keys: &mut [Address], required: usize) -> Result<Vec<u8>, Error> {
    pub_keys.sort();
    multisig_redeem_script_sorted(pub_keys.iter(), required)
}
