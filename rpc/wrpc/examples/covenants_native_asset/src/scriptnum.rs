use crate::errors::CovenantError;

// Fixed-size u64 payload helpers; values must fit i64 for script operations.
pub fn append_u64_le(payload: &mut Vec<u8>, value: u64, field: &'static str) -> Result<(), CovenantError> {
    ensure_u64_fits_i64(value, field)?;
    payload.extend_from_slice(&value.to_le_bytes());
    Ok(())
}

pub fn decode_u64_le(bytes: &[u8], field: &'static str) -> Result<u64, CovenantError> {
    if bytes.len() != 8 {
        return Err(CovenantError::InvalidField(field));
    }
    let value = u64::from_le_bytes(bytes.try_into().map_err(|_| CovenantError::InvalidField(field))?);
    ensure_u64_fits_i64(value, field)?;
    Ok(value)
}

fn ensure_u64_fits_i64(value: u64, field: &'static str) -> Result<(), CovenantError> {
    if value > i64::MAX as u64 {
        return Err(CovenantError::ScriptNumOverflow { field, value });
    }
    Ok(())
}
