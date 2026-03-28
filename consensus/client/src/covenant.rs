//!
//! Client-side WASM wrapper for [`kaspa_consensus_core::tx::GenesisCovenantGroup`].
//!

#![allow(non_snake_case)]

use crate::imports::*;
use crate::result::Result as ClientResult;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::errors::tx::PopulateGenesisCovenantsError;
use kaspa_consensus_core::tx::{CovenantBinding as CoreCovenantBinding, GenesisCovenantGroup as CoreGenesisCovenantGroup};
use kaspa_hashes::Hash;
use kaspa_wasm_core::types::NumberArray;
use workflow_wasm::error::Error as WasmError;

#[wasm_bindgen(typescript_custom_section)]
const TS_COVENANT_BINDING: &'static str = r#"
/**
 * A covenant binding binds a transaction output to the covenant and input authorizing its creation.
 *
 * @category Consensus
 */
export interface ICovenantBinding {
    authorizingInput: number;
    covenantId: HexString;
}
"#;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Copy, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct CovenantBindingInner {
    pub authorizing_input: u16,
    pub covenant_id: Hash,
}

impl CovenantBindingInner {
    pub fn new(authorizing_input: u16, covenant_id: Hash) -> Self {
        Self { authorizing_input, covenant_id }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, CastFromJs)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(inspectable)]
pub struct CovenantBinding {
    inner: Arc<Mutex<CovenantBindingInner>>,
}

impl BorshSerialize for CovenantBinding {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let inner = self.inner.lock().unwrap();
        BorshSerialize::serialize(&*inner, writer)
    }
}

impl BorshDeserialize for CovenantBinding {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let inner = CovenantBindingInner::deserialize_reader(reader)?;
        Ok(Self { inner: Arc::new(Mutex::new(inner)) })
    }
}

impl CovenantBinding {
    pub fn inner(&self) -> MutexGuard<'_, CovenantBindingInner> {
        self.inner.lock().unwrap()
    }
}

#[wasm_bindgen]
impl CovenantBinding {
    #[wasm_bindgen(constructor)]
    pub fn new(authorizing_input: u16, covenant_id: Hash) -> Self {
        let inner = Arc::new(Mutex::new(CovenantBindingInner::new(authorizing_input, covenant_id)));
        Self { inner }
    }

    #[wasm_bindgen(setter, js_name = authorizingInput)]
    pub fn set_authorizing_input(&mut self, v: u16) {
        self.inner().authorizing_input = v;
    }

    #[wasm_bindgen(getter, js_name = authorizingInput)]
    pub fn get_authorizing_input(&self) -> u16 {
        self.inner().authorizing_input
    }

    #[wasm_bindgen(setter, js_name = covenantId)]
    pub fn set_covenant_id(&mut self, v: Hash) {
        self.inner().covenant_id = v;
    }

    #[wasm_bindgen(getter, js_name = covenantId)]
    pub fn get_covenant_id(&self) -> Hash {
        self.inner().covenant_id
    }

    #[wasm_bindgen(js_name = toJSON)]
    pub fn to_js_object(&self) -> Result<Object, WasmError> {
        let obj = Object::new();
        obj.set("authorizingInput", &self.get_authorizing_input().into())?;
        obj.set("covenantId", &self.get_covenant_id().to_string().into())?;
        Ok(obj)
    }
}

impl From<CoreCovenantBinding> for CovenantBinding {
    fn from(covenant: CoreCovenantBinding) -> Self {
        CovenantBinding::new(covenant.authorizing_input, covenant.covenant_id)
    }
}

impl From<CovenantBinding> for CoreCovenantBinding {
    fn from(covenant: CovenantBinding) -> Self {
        CoreCovenantBinding::new(covenant.get_authorizing_input(), covenant.get_covenant_id())
    }
}

impl TryCastFromJs for CovenantBinding {
    type Error = WasmError;

    fn try_cast_from<'a, R>(value: &'a R) -> Result<Cast<'a, Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Self::resolve(value, || {
            let Some(object) = Object::try_from(value.as_ref()) else { return Err(Self::Error::NotAnObject) };
            let authorizing_input = object.get_u16("authorizingInput")?;
            let covenant_id = object.get_value("covenantId")?.try_into_owned()?;
            Ok(CovenantBinding::new(authorizing_input, covenant_id))
        })
    }
}

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
    pub fn new(authorizing_input: u16, outputs: Vec<u32>) -> Self {
        Self { inner: CoreGenesisCovenantGroup::new(authorizing_input, outputs) }
    }

    pub fn inner(&self) -> &CoreGenesisCovenantGroup {
        &self.inner
    }

    pub fn outputs(&self) -> Vec<u32> {
        self.inner.outputs.clone()
    }

    pub fn set_outputs(&mut self, outputs: Vec<u32>) {
        self.inner.outputs = outputs
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

    #[wasm_bindgen(setter = outputs, js_name = otuputs)]
    pub fn js_set_outputs(&mut self, outputs: NumberArray) -> ClientResult<()> {
        let outputs: Vec<u32> = serde_wasm_bindgen::from_value(outputs.into())?;
        self.inner.outputs = outputs;
        Ok(())
    }

    #[wasm_bindgen(getter, js_name = outputs)]
    pub fn js_outputs(&self) -> NumberArray {
        serde_wasm_bindgen::to_value(&self.inner.outputs)
            .expect("serializing genesis covenant outputs should not fail")
            .unchecked_into()
    }

    #[wasm_bindgen(js_name = "toJSON")]
    pub fn to_js_object(&self) -> Result<Object, WasmError> {
        let obj = Object::new();
        obj.set("authorizingInput", &self.inner.authorizing_input.into())?;
        obj.set("outputs", &js_sys::Array::from_iter(self.inner.outputs.iter().map(|&v| JsValue::from(v))))?;
        Ok(obj)
    }

    #[wasm_bindgen(js_name = "toString")]
    pub fn js_to_string(&self) -> Result<js_sys::JsString, WasmError> {
        Ok(js_sys::JSON::stringify(&self.to_js_object()?.into())?)
    }
}

impl TryCastFromJs for GenesisCovenantGroup {
    type Error = WasmError;

    fn try_cast_from<'a, R>(value: &'a R) -> Result<Cast<'a, Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Self::resolve(value, || {
            let Some(object) = Object::try_from(value.as_ref()) else {
                return Err(Self::Error::NotAnObject);
            };
            let authorizing_input = object.get_u16("authorizingInput")?;
            let outputs: Vec<u32> =
                serde_wasm_bindgen::from_value(object.get_value("outputs")?).map_err(|err| Self::Error::from(JsValue::from(err)))?;
            Ok(Self { inner: CoreGenesisCovenantGroup::new(authorizing_input, outputs) })
        })
    }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use super::*;
    use kaspa_hashes::HASH_SIZE;
    use std::str::FromStr;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_test::wasm_bindgen_test;

    // -------------------------------------
    // GenesisCovenantGroup tests

    // Helper - convert &[u32] to a JS Array
    fn to_js_array(values: &[u32]) -> js_sys::Array {
        let arr = js_sys::Array::new();
        for &v in values {
            arr.push(&JsValue::from(v));
        }
        arr
    }

    // Helper - construct GenesisCovenantGroup via WASM constructor
    fn construct_genesis_covenant_group(authorizing_input: u16, outputs: &[u32]) -> GenesisCovenantGroup {
        GenesisCovenantGroup::ctor(authorizing_input, to_js_array(outputs).unchecked_into()).expect("constructor should succeed")
    }

    // Helper - build plain GenesisCovenantGroup JS object
    fn construct_genesis_covenant_group_plain(authorizing_input: u16, outputs: &[u32]) -> JsValue {
        let obj = Object::new();
        obj.set("authorizingInput", &JsValue::from(authorizing_input)).expect("set authorizingInput");
        obj.set("outputs", &to_js_array(outputs).into()).expect("set outputs");
        obj.into()
    }

    // Helper - get outputs field
    fn get_genesis_covenant_group_outputs(group: &GenesisCovenantGroup) -> Vec<u32> {
        serde_wasm_bindgen::from_value(group.outputs().into()).expect("outputs should deserialize")
    }

    #[wasm_bindgen_test]
    fn test_genesis_covenant_group_construction() {
        let group = construct_genesis_covenant_group(1, &[0, 1, 2]);

        assert_eq!(group.authorizing_input(), 1);
        assert_eq!(get_genesis_covenant_group_outputs(&group), vec![0, 1, 2]);
    }

    #[wasm_bindgen_test]
    fn test_genesis_covenant_group_authorizing_input_setter() {
        let mut group = construct_genesis_covenant_group(0, &[0, 1]);

        group.set_authorizing_input(7);
        assert_eq!(group.authorizing_input(), 7);
    }

    #[wasm_bindgen_test]
    fn test_genesis_covenant_group_outputs_setter() {
        let mut group = construct_genesis_covenant_group(0, &[0, 1]);

        group.set_outputs(to_js_array(&[3, 4, 5]).unchecked_into()).expect("set_outputs should succeed");

        assert_eq!(get_genesis_covenant_group_outputs(&group), vec![3, 4, 5]);
    }

    #[wasm_bindgen_test]
    fn test_genesis_covenant_group_to_json() {
        let group = construct_genesis_covenant_group(2, &[0, 1]);

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
    fn test_genesis_covenant_group_try_cast_from_plain_object() {
        let js_val = construct_genesis_covenant_group_plain(1, &[0, 2]);
        let group = GenesisCovenantGroup::try_owned_from(&js_val).expect("try_cast_from should succeed");
        assert_eq!(group.inner().authorizing_input, 1);
        assert_eq!(group.inner().outputs, vec![0, 2]);
    }

    #[wasm_bindgen_test]
    fn test_genesis_covenant_group_array_t_from_plain_objects() {
        let arr = js_sys::Array::new();
        arr.push(&construct_genesis_covenant_group_plain(0, &[0, 1]));
        arr.push(&construct_genesis_covenant_group_plain(1, &[2, 3, 4]));

        let typed_arr: &GenesisCovenantGroupArrayT = arr.unchecked_ref();
        let groups: Vec<CoreGenesisCovenantGroup> = Vec::try_from(typed_arr).expect("should convert from plain objects");

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].authorizing_input, 0);
        assert_eq!(groups[0].outputs, vec![0, 1]);
        assert_eq!(groups[1].authorizing_input, 1);
        assert_eq!(groups[1].outputs, vec![2, 3, 4]);
    }

    #[wasm_bindgen_test]
    fn test_genesis_covenant_group_array_t_from_wasm_instances() {
        let g0 = construct_genesis_covenant_group(5, &[10, 11]);
        let g1 = construct_genesis_covenant_group(6, &[20]);

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

    // -------------------------------------
    // CovenantBinding tests

    #[wasm_bindgen_test]
    fn test_covenant_binding_construction() {
        let covenant_id = Hash::from_bytes([0xab; HASH_SIZE]);
        let binding = CovenantBinding::new(0, covenant_id);

        assert_eq!(binding.get_authorizing_input(), 0);
        assert_eq!(binding.get_covenant_id(), covenant_id);
    }

    #[wasm_bindgen_test]
    fn test_covenant_binding_construction_plain() {
        let covenant_id = Hash::from_bytes([0xab; HASH_SIZE]);

        let obj = Object::new();
        obj.set("authorizingInput", &JsValue::from(0)).expect("set authorizingInput");
        obj.set("covenantId", &JsValue::from_str(&covenant_id.to_string())).expect("set covenantId");

        let binding = CovenantBinding::try_owned_from(obj).expect("try_cast_from failed");

        assert_eq!(binding.get_authorizing_input(), 0);
        assert_eq!(binding.get_covenant_id(), covenant_id);
    }

    #[wasm_bindgen_test]
    fn test_covenant_binding_to_json() {
        let covenant_id_in = Hash::from_bytes([0xab; HASH_SIZE]);
        let binding = CovenantBinding::new(0, covenant_id_in);

        let obj = binding.to_js_object().expect("to_js_object failed");

        let authorizing_input_out = obj.get_u16("authorizingInput").expect("failed to get authorizingInput");
        let covenant_id_out =
            Hash::from_str(&obj.get_string("covenantId").expect("failed to get covenantId")).expect("failed to create Hash from str");

        assert_eq!(0, authorizing_input_out);
        assert_eq!(covenant_id_in, covenant_id_out);
    }

    #[wasm_bindgen_test]
    fn test_covenant_binding_authorizing_input_setter() {
        let covenant_id = Hash::from_bytes([0xab; HASH_SIZE]);
        let mut binding = CovenantBinding::new(0, covenant_id);

        let new_input = 1;

        binding.set_authorizing_input(new_input);

        assert_eq!(binding.get_authorizing_input(), new_input);
    }

    #[wasm_bindgen_test]
    fn test_covenant_binding_covenant_id_setter() {
        let covenant_id = Hash::from_bytes([0xab; HASH_SIZE]);
        let mut binding = CovenantBinding::new(0, covenant_id);

        let new_covenant_id = Hash::from_bytes([0xac; HASH_SIZE]);

        binding.set_covenant_id(new_covenant_id);

        assert_eq!(binding.get_covenant_id(), new_covenant_id);
    }
}
