//!
//! WASM implementation of a CompressedParents.
//! This struct is composed of primitives and other WASM types.
//!

use crate::imports::*;
use kaspa_consensus_core::header as native;
use kaspa_hashes::Hash;

/// An efficient cumulative-sum run-length encoding for the parents-by-level vector in the block header.
/// @category Consensus
#[derive(Clone, Debug, Serialize, Deserialize, CastFromJs)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(inspectable)]
pub struct CompressedParents {
    inner: native::CompressedParents,
}

#[wasm_bindgen]
impl CompressedParents {
    #[wasm_bindgen(constructor)]
    pub fn new(js_value: JsValue) -> Result<CompressedParents, JsError> {
        let inner = serde_wasm_bindgen::from_value(js_value)?;
        Ok(CompressedParents { inner })
    }

    /// Get the parent hashes at a specific level.
    /// Returns an array of `HexString`s.
    #[wasm_bindgen]
    pub fn get(&self, index: usize) -> Result<JsValue, JsError> {
        Ok(serde_wasm_bindgen::to_value(&self.inner.get(index))?)
    }

    /// The number of levels in the expanded representation.
    #[wasm_bindgen(js_name = expandedLen)]
    pub fn expanded_len(&self) -> usize {
        self.inner.expanded_len()
    }

    /// Converts the compressed parents to an expanded `JsValue` of `Array<Array<HexString>>`.
    #[wasm_bindgen(js_name = toExpanded)]
    pub fn to_expanded(&self) -> Result<JsValue, JsError> {
        let expanded: Vec<Vec<Hash>> = self.inner.clone().into();
        Ok(serde_wasm_bindgen::to_value(&expanded)?)
    }
}

impl From<native::CompressedParents> for CompressedParents {
    fn from(inner: native::CompressedParents) -> Self {
        Self { inner }
    }
}

impl From<CompressedParents> for native::CompressedParents {
    fn from(wrapped: CompressedParents) -> Self {
        wrapped.inner
    }
}

impl TryCastFromJs for CompressedParents {
    type Error = Error;
    fn try_cast_from<'a, R>(value: &'a R) -> Result<Cast<'a, Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Self::resolve_cast(value, || {
            let array = js_sys::Array::from(value.as_ref());
            let runs = array
                .iter()
                .map(|run_js| {
                    let run_array = js_sys::Array::from(&run_js);
                    if run_array.length() != 2 {
                        return Err(Error::Custom("Invalid parent run, must be a tuple of [level, parents[]]".to_string()));
                    }
                    let level = run_array.get(0).try_as_u8().map_err(|_| Error::Custom("Invalid level in parent runs".to_string()))?;

                    let parents_js = run_array.get(1);
                    let parents_array = js_sys::Array::from(&parents_js);
                    let parents =
                        parents_array.iter().map(|hash_js| hash_js.try_into_owned()).collect::<std::result::Result<Vec<Hash>, _>>()?;

                    Ok((level, parents))
                })
                .collect::<std::result::Result<Vec<(u8, Vec<Hash>)>, Error>>()?;

            Ok(Self { inner: runs.try_into()? }.into())
        })
    }
}
