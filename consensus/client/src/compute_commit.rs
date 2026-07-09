//!
//! Client-side WASM wrapper for [`kaspa_consensus_core::tx::ComputeCommit`].
//!

#![allow(non_snake_case)]

use crate::error::Error as ClientError;
use crate::imports::*;
use crate::result::Result as ClientResult;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::mass::{ComputeBudget, SigopCount};
use kaspa_consensus_core::tx::ComputeCommit as CoreComputeCommit;
use workflow_wasm::error::Error as WasmError;

const COMPUTE_COMMIT_TYPE_SIG_OP_COUNT: &str = "sigOpCount";
const COMPUTE_COMMIT_TYPE_COMPUTE_BUDGET: &str = "computeBudget";

#[wasm_bindgen(typescript_custom_section)]
const TS_COMPUTE_COMMIT: &'static str = r#"
/**
 * A compute commit encodes the input script execution budget committed by a transaction.
 *
 * Version 0 transactions use `sigOpCount`; version >= 1 transactions use `computeBudget`.
 *
 * @category Consensus
 */
export type ComputeCommitType = "sigOpCount" | "computeBudget";

/**
 * @category Consensus
 */
export interface IComputeCommit {
    type: ComputeCommitType;
    value: number;
}
"#;

#[wasm_bindgen]
extern "C" {
    /// WASM (TypeScript) type representing `IComputeCommit | ComputeCommit`.
    /// @category Consensus
    #[wasm_bindgen(typescript_type = "IComputeCommit | ComputeCommit")]
    pub type ComputeCommitT;
}

/// Encodes the compute mass commitment for a transaction input.
/// @category Consensus
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, CastFromJs)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(inspectable)]
pub struct ComputeCommit {
    inner: CoreComputeCommit,
}

impl ComputeCommit {
    pub fn inner(&self) -> CoreComputeCommit {
        self.inner
    }

    pub fn set_inner(&mut self, inner: CoreComputeCommit) {
        self.inner = inner;
    }
}

#[wasm_bindgen]
impl ComputeCommit {
    #[wasm_bindgen(constructor)]
    pub fn constructor(value: &ComputeCommitT) -> ClientResult<Self> {
        Ok(Self::try_owned_from(value)?)
    }

    #[wasm_bindgen(js_name = fromSigOpCount)]
    pub fn from_sig_op_count(sig_op_count: u8) -> Self {
        Self { inner: CoreComputeCommit::SigopCount(SigopCount::from(sig_op_count)) }
    }

    #[wasm_bindgen(js_name = fromComputeBudget)]
    pub fn from_compute_budget(compute_budget: u16) -> Self {
        Self { inner: CoreComputeCommit::ComputeBudget(ComputeBudget::from(compute_budget)) }
    }

    #[wasm_bindgen(getter = type)]
    pub fn get_type(&self) -> String {
        match self.inner {
            CoreComputeCommit::SigopCount(_) => COMPUTE_COMMIT_TYPE_SIG_OP_COUNT,
            CoreComputeCommit::ComputeBudget(_) => COMPUTE_COMMIT_TYPE_COMPUTE_BUDGET,
        }
        .to_string()
    }

    #[wasm_bindgen(setter = type)]
    pub fn set_type(&mut self, value: &str) -> ClientResult<()> {
        self.inner = match value {
            COMPUTE_COMMIT_TYPE_SIG_OP_COUNT => CoreComputeCommit::SigopCount(SigopCount::from(0)),
            COMPUTE_COMMIT_TYPE_COMPUTE_BUDGET => CoreComputeCommit::ComputeBudget(ComputeBudget::from(0)),
            _ => return Err(ClientError::custom(format!("invalid compute commit type: {value}"))),
        };
        Ok(())
    }

    #[wasm_bindgen(getter = value)]
    pub fn get_value(&self) -> u16 {
        match self.inner {
            CoreComputeCommit::SigopCount(sig_op_count) => u8::from(sig_op_count) as u16,
            CoreComputeCommit::ComputeBudget(compute_budget) => u16::from(compute_budget),
        }
    }

    #[wasm_bindgen(setter = value)]
    pub fn set_value(&mut self, value: u16) -> ClientResult<()> {
        self.inner = match self.inner {
            CoreComputeCommit::SigopCount(_) => {
                let sig_op_count = u8::try_from(value).map_err(|_| ClientError::custom("sigOpCount must fit in 0..=255"))?;
                CoreComputeCommit::SigopCount(SigopCount::from(sig_op_count))
            }
            CoreComputeCommit::ComputeBudget(_) => CoreComputeCommit::ComputeBudget(ComputeBudget::from(value)),
        };
        Ok(())
    }
}

impl From<CoreComputeCommit> for ComputeCommit {
    fn from(inner: CoreComputeCommit) -> Self {
        Self { inner }
    }
}

impl From<ComputeCommit> for CoreComputeCommit {
    fn from(commit: ComputeCommit) -> Self {
        commit.inner
    }
}

impl TryCastFromJs for ComputeCommit {
    type Error = WasmError;

    fn try_cast_from<'a, R>(value: &'a R) -> std::result::Result<Cast<'a, Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Self::resolve(value, || {
            let Some(object) = Object::try_from(value.as_ref()) else {
                return Err(Self::Error::NotAnObject);
            };
            let commit_type = object.get_string("type")?;
            let value = object.get_u16("value")?;
            Ok(match commit_type.as_str() {
                COMPUTE_COMMIT_TYPE_SIG_OP_COUNT => {
                    let sig_op_count = u8::try_from(value).map_err(|_| WasmError::custom("sigOpCount must fit in u8"))?;
                    ComputeCommit::from_sig_op_count(sig_op_count)
                }
                COMPUTE_COMMIT_TYPE_COMPUTE_BUDGET => ComputeCommit::from_compute_budget(value),
                _ => return Err(WasmError::custom(format!("invalid compute commit type: {commit_type}"))),
            })
        })
    }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use super::*;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn test_compute_commit_sig_op_count_construction() {
        let commit = ComputeCommit::from_sig_op_count(3);

        assert_eq!(commit.get_type(), COMPUTE_COMMIT_TYPE_SIG_OP_COUNT);
        assert_eq!(commit.get_value(), 3);
        assert_eq!(commit.inner(), CoreComputeCommit::SigopCount(SigopCount::from(3)));
    }

    #[wasm_bindgen_test]
    fn test_compute_commit_compute_budget_construction() {
        let commit = ComputeCommit::from_compute_budget(1234);

        assert_eq!(commit.get_type(), COMPUTE_COMMIT_TYPE_COMPUTE_BUDGET);
        assert_eq!(commit.get_value(), 1234);
        assert_eq!(commit.inner(), CoreComputeCommit::ComputeBudget(ComputeBudget::from(1234)));
    }

    #[wasm_bindgen_test]
    fn test_compute_commit_try_cast_from_plain_object() {
        let obj = Object::new();
        obj.set("type", &JsValue::from_str(COMPUTE_COMMIT_TYPE_COMPUTE_BUDGET)).expect("set type");
        obj.set("value", &JsValue::from(99)).expect("set value");

        let commit = ComputeCommit::try_owned_from(obj).expect("try_cast_from failed");

        assert_eq!(commit.inner(), CoreComputeCommit::ComputeBudget(ComputeBudget::from(99)));
    }
}
