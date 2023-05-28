use js_sys::BigInt;
use kaspa_math::Uint256;
use kaspa_utils::hex::ToHex;
use num::Float;
use wasm_bindgen::prelude::*;

// https://github.com/tmrlvi/kaspa-miner/blob/bf361d02a46c580f55f46b5dfa773477634a5753/src/client/stratum.rs#L36
const DIFFICULTY_1_TARGET: (u64, i16) = (0xffffu64, 208); // 0xffff 2^208

/// `calculate_difficulty` is based on set_difficulty function: https://github.com/tmrlvi/kaspa-miner/blob/bf361d02a46c580f55f46b5dfa773477634a5753/src/client/stratum.rs#L375
#[wasm_bindgen(js_name = calculateDifficulty)]
pub fn calculate_difficulty(difficulty: f32) -> Result<BigInt, JsError> {
    let mut buf = [0u64, 0u64, 0u64, 0u64];
    let (mantissa, exponent, _) = difficulty.recip().integer_decode();
    let new_mantissa = mantissa * DIFFICULTY_1_TARGET.0;
    let new_exponent = (DIFFICULTY_1_TARGET.1 + exponent) as u64;
    let start = (new_exponent / 64) as usize;
    let remainder = new_exponent % 64;

    buf[start] = new_mantissa << remainder; // bottom
    if start < 3 {
        buf[start + 1] = new_mantissa >> (64 - remainder); // top
    } else if new_mantissa.leading_zeros() < remainder as u32 {
        return Err(JsError::new("Target is too big"));
    }

    // let target_pool = Uint256(buf);
    // workflow_log::log_info!("Difficulty: {:?}, Target: 0x{}", difficulty, target_pool.to_hex());
    Ok(Uint256(buf).try_into()?)
}
