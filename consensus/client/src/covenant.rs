//!
//! Client-side WASM wrapper for [`kaspa_consensus_core::tx::GenesisCovenantGroup`].
//!

#![allow(non_snake_case)]

use crate::imports::*;
use kaspa_consensus_core::errors::tx::PopulateGenesisCovenantsError;
use kaspa_consensus_core::tx::GenesisCovenantGroup as CoreGenesisCovenantGroup;

#[wasm_bindgen]
extern "C" {
    /// WASM (TypeScript) type representing an array of [`GenesisCovenantGroup`] objects: `GenesisCovenantGroup[]`.
    ///
    /// @category Consensus
    #[wasm_bindgen(extends = js_sys::Array, typescript_type = "GenesisCovenantGroup[]")]
    pub type GenesisCovenantGroupArrayT;
}

impl TryFrom<&GenesisCovenantGroupArrayT> for Vec<CoreGenesisCovenantGroup> {
    type Error = PopulateGenesisCovenantsError;
    fn try_from(value: &GenesisCovenantGroupArrayT) -> Result<Self, Self::Error> {
        if value.is_array() {
            value
                .iter()
                .map(|v| {
                    GenesisCovenantGroup::try_owned_from(v).map(|gcp| gcp.inner().clone()).map_err(PopulateGenesisCovenantsError::from)
                })
                .collect()
        } else {
            Err(PopulateGenesisCovenantsError::InvalidGenesisCovenantGroupArray)
        }
    }
}

/// A genesis covenant group for bulk covenant binding population.
///
/// All listed outputs are bound to the same covenant id, derived from the
/// authorizing input outpoint and this exact ordered output list.
/// @category Consensus
#[derive(Debug, Clone, Serialize, Deserialize, CastFromJs)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(inspectable)]
pub struct GenesisCovenantGroup {
    inner: CoreGenesisCovenantGroup,
}

impl GenesisCovenantGroup {
    pub fn inner(&self) -> &CoreGenesisCovenantGroup {
        &self.inner
    }
}

impl From<CoreGenesisCovenantGroup> for GenesisCovenantGroup {
    fn from(inner: CoreGenesisCovenantGroup) -> Self {
        Self { inner }
    }
}

impl From<GenesisCovenantGroup> for CoreGenesisCovenantGroup {
    fn from(wrapper: GenesisCovenantGroup) -> Self {
        wrapper.inner
    }
}

#[wasm_bindgen]
impl GenesisCovenantGroup {
    #[wasm_bindgen(constructor)]
    pub fn ctor(authorizing_input: u16, outputs: Vec<u32>) -> Self {
        Self { inner: CoreGenesisCovenantGroup::new(authorizing_input, outputs) }
    }

    #[wasm_bindgen(getter, js_name = authorizingInput)]
    pub fn authorizing_input(&self) -> u16 {
        self.inner.authorizing_input
    }

    #[wasm_bindgen(setter, js_name = authorizingInput)]
    pub fn set_authorizing_input(&mut self, value: u16) {
        self.inner.authorizing_input = value;
    }

    #[wasm_bindgen(setter = outputs)]
    pub fn set_outputs(&mut self, outputs: Vec<u32>) {
        self.inner.outputs = outputs;
    }

    #[wasm_bindgen(getter)]
    pub fn outputs(&self) -> Vec<u32> {
        self.inner.outputs.clone()
    }

    #[wasm_bindgen(js_name = "toJSON")]
    pub fn to_js_object(&self) -> Result<Object, workflow_wasm::error::Error> {
        let obj = Object::new();
        obj.set("authorizingInput", &self.inner.authorizing_input.into())?;
        obj.set("outputs", &js_sys::Array::from_iter(self.inner.outputs.iter().map(|&v| JsValue::from(v))))?;
        Ok(obj)
    }

    #[wasm_bindgen(js_name = "toString")]
    pub fn js_to_string(&self) -> Result<js_sys::JsString, workflow_wasm::error::Error> {
        Ok(js_sys::JSON::stringify(&self.to_js_object()?.into())?)
    }
}

impl TryCastFromJs for GenesisCovenantGroup {
    type Error = workflow_wasm::error::Error;

    fn try_cast_from<'a, R>(value: &'a R) -> Result<Cast<'a, Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Self::resolve(value, || {
            let Some(object) = Object::try_from(value.as_ref()) else {
                return Err(workflow_wasm::error::Error::NotAnObject);
            };
            let authorizing_input = object.get_u16("authorizingInput")?;
            let outputs = object
                .get_vec("outputs")?
                .iter()
                .map(|idx| idx.try_as_u32())
                .collect::<Result<Vec<u32>, workflow_wasm::error::Error>>()?;
            Ok(Self { inner: CoreGenesisCovenantGroup::new(authorizing_input, outputs) })
        })
    }
}
