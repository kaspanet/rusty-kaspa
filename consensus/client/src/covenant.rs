//!
//! Client-side WASM wrapper for [`kaspa_consensus_core::tx::GenesisCovenantGroup`].
//!

#![allow(non_snake_case)]

use crate::imports::*;
use crate::result::Result as ClientResult;
use kaspa_consensus_core::errors::tx::PopulateGenesisCovenantsError;
use kaspa_consensus_core::tx::GenesisCovenantGroup as CoreGenesisCovenantGroup;
use kaspa_wasm_core::types::NumberArray;

#[wasm_bindgen(typescript_custom_section)]
const TS_GENESIS_COVENANT_GROUP: &'static str = r#"
/**
 * A genesis covenant group for bulk covenant binding population.
 *
 * @category Consensus
 */
export interface IGenesisCovenantGroup {
    authorizingInput: number;
    outputs: number[];
}
"#;

#[wasm_bindgen]
extern "C" {
    /// WASM (TypeScript) type representing an array of [`GenesisCovenantGroup`] objects.
    ///
    /// @category Consensus
    #[wasm_bindgen(extends = js_sys::Array, typescript_type = "(IGenesisCovenantGroup | GenesisCovenantGroup)[]")]
    pub type GenesisCovenantGroupArrayT;
}

impl TryFrom<&GenesisCovenantGroupArrayT> for Vec<CoreGenesisCovenantGroup> {
    type Error = Error;
    fn try_from(value: &GenesisCovenantGroupArrayT) -> Result<Self, Self::Error> {
        if value.is_array() {
            value.iter().map(|v| GenesisCovenantGroup::try_owned_from(v).map(|gcp| gcp.inner().clone()).map_err(Error::from)).collect()
        } else {
            Err(PopulateGenesisCovenantsError::InvalidGenesisCovenantGroupArray.into())
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
    pub fn ctor(authorizing_input: u16, outputs: NumberArray) -> ClientResult<Self> {
        let outputs: Vec<u32> = serde_wasm_bindgen::from_value(outputs.into())?;
        Ok(Self { inner: CoreGenesisCovenantGroup::new(authorizing_input, outputs) })
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
    pub fn set_outputs(&mut self, outputs: NumberArray) -> ClientResult<()> {
        let outputs: Vec<u32> = serde_wasm_bindgen::from_value(outputs.into())?;
        self.inner.outputs = outputs;
        Ok(())
    }

    #[wasm_bindgen(getter)]
    pub fn outputs(&self) -> NumberArray {
        serde_wasm_bindgen::to_value(&self.inner.outputs)
            .expect("serializing genesis covenant outputs should not fail")
            .unchecked_into()
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
            let outputs: Vec<u32> = serde_wasm_bindgen::from_value(object.get_value("outputs")?)
                .map_err(|err| workflow_wasm::error::Error::from(JsValue::from(err)))?;
            Ok(Self { inner: CoreGenesisCovenantGroup::new(authorizing_input, outputs) })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_test::wasm_bindgen_test;

    /// Helper - convert &[u32] to a JS Array
    fn _to_js_array(values: &[u32]) -> js_sys::Array {
        let arr = js_sys::Array::new();
        for &v in values {
            arr.push(&JsValue::from(v));
        }
        arr
    }

    /// Helper - construct GenesisCovenantGroup via WASM constructor
    fn _construct_covenant_group(authorizing_input: u16, outputs: &[u32]) -> GenesisCovenantGroup {
        GenesisCovenantGroup::ctor(authorizing_input, _to_js_array(outputs).unchecked_into()).expect("constructor should succeed")
    }

    /// Helper - build plain GenesisCovenantGroup JS object
    fn _construct_covenant_group_plain(authorizing_input: u16, outputs: &[u32]) -> JsValue {
        let obj = Object::new();
        obj.set("authorizingInput", &JsValue::from(authorizing_input)).expect("set authorizingInput");
        obj.set("outputs", &_to_js_array(outputs).into()).expect("set outputs");
        obj.into()
    }

    /// Helper - get outputs field
    fn _get_outputs(group: &GenesisCovenantGroup) -> Vec<u32> {
        serde_wasm_bindgen::from_value(group.outputs().into()).expect("outputs should deserialize")
    }

    #[wasm_bindgen_test]
    fn _test_genesis_covenant_group_construction() {
        let group = _construct_covenant_group(1, &[0, 1, 2]);

        assert_eq!(group.authorizing_input(), 1);
        assert_eq!(_get_outputs(&group), vec![0, 1, 2]);
    }

    #[wasm_bindgen_test]
    fn _test_authorizing_input_setter() {
        let mut group = _construct_covenant_group(0, &[0, 1]);

        group.set_authorizing_input(7);
        assert_eq!(group.authorizing_input(), 7);
    }

    #[wasm_bindgen_test]
    fn _test_outputs_setter() {
        let mut group = _construct_covenant_group(0, &[0, 1]);

        group.set_outputs(_to_js_array(&[3, 4, 5]).unchecked_into()).expect("set_outputs should succeed");

        assert_eq!(_get_outputs(&group), vec![3, 4, 5]);
    }

    #[wasm_bindgen_test]
    fn _test_to_json() {
        let group = _construct_covenant_group(2, &[0, 1]);

        let obj = group.to_js_object().expect("to_js_object should succeed");
        let auth_input = js_sys::Reflect::get(&obj, &JsValue::from_str("authorizingInput")).expect("should have authorizingInput");
        assert_eq!(auth_input.as_f64().expect("should be a number"), 2.0);

        let outputs = js_sys::Reflect::get(&obj, &JsValue::from_str("outputs")).expect("should have outputs");
        let outputs_array: &js_sys::Array = outputs.dyn_ref::<js_sys::Array>().expect("outputs should be an array");
        assert_eq!(outputs_array.length(), 2);
        assert_eq!(outputs_array.get(0).as_f64().expect("should be a number"), 0.0);
        assert_eq!(outputs_array.get(1).as_f64().expect("should be a number"), 1.0);

        // Also verify toString() round-trips through JSON.stringify correctly
        let json_str: String = group.js_to_string().expect("js_to_string should succeed").into();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("should be valid JSON");
        assert_eq!(parsed["authorizingInput"], 2);
        assert_eq!(parsed["outputs"], serde_json::json!([0, 1]));
    }

    #[wasm_bindgen_test]
    fn _test_try_cast_from_plain_object() {
        let js_val = _construct_covenant_group_plain(1, &[0, 2]);
        let group = GenesisCovenantGroup::try_owned_from(&js_val).expect("try_cast_from should succeed");
        assert_eq!(group.inner().authorizing_input, 1);
        assert_eq!(group.inner().outputs, vec![0, 2]);
    }

    #[wasm_bindgen_test]
    fn _test_empty_outputs_construction() {
        let group = _construct_covenant_group(0, &[]);
        assert!(_get_outputs(&group).is_empty());
    }

    #[wasm_bindgen_test]
    fn _test_array_t_from_plain_objects() {
        let arr = js_sys::Array::new();
        arr.push(&_construct_covenant_group_plain(0, &[0, 1]));
        arr.push(&_construct_covenant_group_plain(1, &[2, 3, 4]));

        let typed_arr: &GenesisCovenantGroupArrayT = arr.unchecked_ref();
        let groups: Vec<CoreGenesisCovenantGroup> = Vec::try_from(typed_arr).expect("should convert from plain objects");

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].authorizing_input, 0);
        assert_eq!(groups[0].outputs, vec![0, 1]);
        assert_eq!(groups[1].authorizing_input, 1);
        assert_eq!(groups[1].outputs, vec![2, 3, 4]);
    }

    #[wasm_bindgen_test]
    fn _test_array_t_from_wasm_instances() {
        let g0 = _construct_covenant_group(5, &[10, 11]);
        let g1 = _construct_covenant_group(6, &[20]);

        let arr = js_sys::Array::new();
        arr.push(&JsValue::from(g0));
        arr.push(&JsValue::from(g1));

        let typed_arr: &GenesisCovenantGroupArrayT = arr.unchecked_ref();
        let groups: Vec<CoreGenesisCovenantGroup> = Vec::try_from(typed_arr).expect("should convert from WASM instances");

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].authorizing_input, 5);
        assert_eq!(groups[0].outputs, vec![10, 11]);
        assert_eq!(groups[1].authorizing_input, 6);
        assert_eq!(groups[1].outputs, vec![20]);
    }

    #[wasm_bindgen_test]
    fn _test_ctor_rejects_invalid_outputs() {
        let result = GenesisCovenantGroup::ctor(0, JsValue::from_str("not an array").unchecked_into());
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    fn _test_set_outputs_rejects_invalid_value() {
        let mut group = _construct_covenant_group(0, &[0, 1]);
        let result = group.set_outputs(JsValue::from_str("bad").unchecked_into());
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    fn _test_try_cast_rejects_non_object() {
        let val = JsValue::from(42);
        let result = GenesisCovenantGroup::try_owned_from(&val);
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    fn _test_try_cast_rejects_missing_fields() {
        let obj = Object::new();
        obj.set("outputs", &_to_js_array(&[0, 1]).into()).expect("set outputs");
        let js_val: JsValue = obj.into();
        let result = GenesisCovenantGroup::try_owned_from(&js_val);
        assert!(result.is_err());
    }

    #[wasm_bindgen_test]
    fn _test_array_t_rejects_non_array() {
        let obj = Object::new();
        let typed: &GenesisCovenantGroupArrayT = obj.unchecked_ref();
        let result = Vec::<CoreGenesisCovenantGroup>::try_from(typed);
        assert!(result.is_err());
    }
}
