use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use kaspa_utils::{
    hex::{FromHex, ToHex},
    serde_bytes::FromHexVisitor,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smallvec::SmallVec;
use std::{
    collections::HashSet,
    str::{self, FromStr},
};
use wasm_bindgen::prelude::*;
use workflow_wasm::jsvalue::JsValueTrait;

/// Size of the underlying script vector of a script.
pub const SCRIPT_VECTOR_SIZE: usize = 36;

/// Used as the underlying type for script public key data, optimized for the common p2pk script size (34).
pub type ScriptVec = SmallVec<[u8; SCRIPT_VECTOR_SIZE]>;

/// Represents the ScriptPublicKey Version
pub type ScriptPublicKeyVersion = u16;

/// Alias the `smallvec!` macro to ease maintenance
pub use smallvec::smallvec as scriptvec;
use wasm_bindgen::prelude::wasm_bindgen;

//Represents a Set of [`ScriptPublicKey`]s
pub type ScriptPublicKeys = HashSet<ScriptPublicKey>;

/// Represents a Kaspad ScriptPublicKey
#[derive(Default, Debug, PartialEq, Eq, Clone, Hash)]
#[wasm_bindgen(inspectable)]
pub struct ScriptPublicKey {
    pub version: ScriptPublicKeyVersion,
    pub(super) script: ScriptVec, // Kept private to preserve read-only semantics
}

impl FromHex for ScriptPublicKey {
    type Error = faster_hex::Error;

    fn from_hex(hex_str: &str) -> Result<Self, Self::Error> {
        ScriptPublicKey::from_str(hex_str)
    }
}

#[derive(Default, Debug, PartialEq, Eq, Serialize, Deserialize, Clone, Hash)]
#[serde(rename_all = "camelCase")]
#[serde(rename = "ScriptPublicKey")]
struct ScriptPublicKeyInternal<'a> {
    version: ScriptPublicKeyVersion,
    script: &'a [u8],
}

impl Serialize for ScriptPublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            let mut hex = vec![0u8; self.script.len() * 2 + 4];
            faster_hex::hex_encode(&self.version.to_be_bytes(), &mut hex).map_err(serde::ser::Error::custom)?;
            faster_hex::hex_encode(&self.script, &mut hex[4..]).map_err(serde::ser::Error::custom)?;
            serializer.serialize_str(unsafe { str::from_utf8_unchecked(&hex) })
        } else {
            ScriptPublicKeyInternal { version: self.version, script: &self.script }.serialize(serializer)
        }
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for ScriptPublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            deserializer.deserialize_str(FromHexVisitor::default())
        } else {
            ScriptPublicKeyInternal::deserialize(deserializer)
                .map(|ScriptPublicKeyInternal { script, version }| Self { version, script: SmallVec::from_slice(script) })
        }
    }
}

impl FromStr for ScriptPublicKey {
    type Err = faster_hex::Error;
    fn from_str(hex_str: &str) -> Result<Self, Self::Err> {
        let hex_len = hex_str.len();
        if hex_len < 4 {
            return Err(faster_hex::Error::InvalidLength(hex_len));
        }
        let mut bytes = vec![0u8; hex_len / 2];
        faster_hex::hex_decode(hex_str.as_bytes(), bytes.as_mut_slice())?;
        let version = u16::from_be_bytes(bytes[0..2].try_into().unwrap());
        Ok(Self { version, script: SmallVec::from_slice(&bytes[2..]) })
    }
}

impl ScriptPublicKey {
    pub fn new(version: ScriptPublicKeyVersion, script: ScriptVec) -> Self {
        Self { version, script }
    }

    pub fn from_vec(version: ScriptPublicKeyVersion, script: Vec<u8>) -> Self {
        Self { version, script: ScriptVec::from_vec(script) }
    }

    pub fn version(&self) -> ScriptPublicKeyVersion {
        self.version
    }

    pub fn script(&self) -> &[u8] {
        &self.script
    }
}

#[wasm_bindgen]
impl ScriptPublicKey {
    #[wasm_bindgen(constructor)]
    pub fn constructor(version: u16, script: JsValue) -> Result<ScriptPublicKey, JsError> {
        let script = script.try_as_vec_u8()?;
        Ok(ScriptPublicKey::new(version, script.into()))
    }

    #[wasm_bindgen(getter = script)]
    pub fn script_as_hex(&self) -> String {
        self.script.to_hex()
    }
}

//
// Borsh serializers need to be manually implemented for `ScriptPublicKey` since
// smallvec does not currently support Borsh
//

impl BorshSerialize for ScriptPublicKey {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        borsh::BorshSerialize::serialize(&self.version, writer)?;
        // Vectors and slices are all serialized internally the same way
        borsh::BorshSerialize::serialize(&self.script.as_slice(), writer)?;
        Ok(())
    }
}

impl BorshDeserialize for ScriptPublicKey {
    fn deserialize(buf: &mut &[u8]) -> std::io::Result<Self> {
        // Deserialize into vec first since we have no custom smallvec support
        Ok(Self::from_vec(borsh::BorshDeserialize::deserialize(buf)?, borsh::BorshDeserialize::deserialize(buf)?))
    }
}

impl BorshSchema for ScriptPublicKey {
    fn add_definitions_recursively(
        definitions: &mut std::collections::HashMap<borsh::schema::Declaration, borsh::schema::Definition>,
    ) {
        let fields = borsh::schema::Fields::NamedFields(std::vec![
            ("version".to_string(), <u16>::declaration()),
            ("script".to_string(), <Vec<u8>>::declaration())
        ]);
        let definition = borsh::schema::Definition::Struct { fields };
        Self::add_definition(Self::declaration(), definition, definitions);
        <u16>::add_definitions_recursively(definitions);
        // `<Vec<u8>>` can be safely used as scheme definition for smallvec. See comments above.
        <Vec<u8>>::add_definitions_recursively(definitions);
    }

    fn declaration() -> borsh::schema::Declaration {
        "ScriptPublicKey".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spk_serde_json() {
        let vec = (0..SCRIPT_VECTOR_SIZE as u8).collect::<Vec<_>>();
        let spk = ScriptPublicKey::from_vec(0xc0de, vec.clone());
        let hex: String = serde_json::to_string(&spk).unwrap();
        assert_eq!("\"c0de000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20212223\"", hex);
        let spk = serde_json::from_str::<ScriptPublicKey>(&hex).unwrap();
        assert_eq!(spk.version, 0xc0de);
        assert_eq!(spk.script.as_slice(), vec.as_slice());
        let result = "00".parse::<ScriptPublicKey>();
        assert!(matches!(result, Err(faster_hex::Error::InvalidLength(2))));
        let result = "0000".parse::<ScriptPublicKey>();
        let _empty = ScriptPublicKey { version: 0, script: ScriptVec::new() };
        assert!(matches!(result, Ok(_empty)));
    }

    #[test]
    fn test_spk_borsh() {
        // Tests for ScriptPublicKey Borsh ser/deser since we manually implemented them
        let spk = ScriptPublicKey::from_vec(12, vec![32; 20]);
        let bin = spk.try_to_vec().unwrap();
        let spk2: ScriptPublicKey = BorshDeserialize::try_from_slice(&bin).unwrap();
        assert_eq!(spk, spk2);

        let spk = ScriptPublicKey::from_vec(55455, vec![11; 200]);
        let bin = spk.try_to_vec().unwrap();
        let spk2: ScriptPublicKey = BorshDeserialize::try_from_slice(&bin).unwrap();
        assert_eq!(spk, spk2);
    }

    // use wasm_bindgen_test::wasm_bindgen_test;
    // #[wasm_bindgen_test]
    // pub fn test_wasm_serde_spk_constructor() {
    //     let str = "kaspa:qpauqsvk7yf9unexwmxsnmg547mhyga37csh0kj53q6xxgl24ydxjsgzthw5j";
    //     let a = Address::constructor(str);
    //     let value = to_value(&a).unwrap();
    //
    //     assert_eq!(JsValue::from_str("string"), value.js_typeof());
    //     assert_eq!(value, JsValue::from_str(str));
    //     assert_eq!(a, from_value(value).unwrap());
    // }
    //
    // #[wasm_bindgen_test]
    // pub fn test_wasm_js_serde_spk_object() {
    //     let expected = Address::constructor("kaspa:qpauqsvk7yf9unexwmxsnmg547mhyga37csh0kj53q6xxgl24ydxjsgzthw5j");
    //
    //     use web_sys::console;
    //     console::log_4(&"address: ".into(), &expected.version().into(), &expected.prefix().into(), &expected.payload().into());
    //
    //     let obj = Object::new();
    //     obj.set("version", &JsValue::from_str("PubKey")).unwrap();
    //     obj.set("prefix", &JsValue::from_str("kaspa")).unwrap();
    //     obj.set("payload", &JsValue::from_str("qpauqsvk7yf9unexwmxsnmg547mhyga37csh0kj53q6xxgl24ydxjsgzthw5j")).unwrap();
    //
    //     assert_eq!(JsValue::from_str("object"), obj.js_typeof());
    //
    //     let obj_js = obj.into_js_result().unwrap();
    //     let actual = from_value(obj_js).unwrap();
    //     assert_eq!(expected, actual);
    // }
    //
    // #[wasm_bindgen_test]
    // pub fn test_wasm_serde_spk_object() {
    //     use wasm_bindgen::convert::IntoWasmAbi;
    //
    //     let expected = Address::constructor("kaspa:qpauqsvk7yf9unexwmxsnmg547mhyga37csh0kj53q6xxgl24ydxjsgzthw5j");
    //     let wasm_js_value: JsValue = expected.clone().into_abi().into();
    //
    //     // use web_sys::console;
    //     // console::log_4(&"address: ".into(), &expected.version().into(), &expected.prefix().into(), &expected.payload().into());
    //
    //     let actual = from_value(wasm_js_value).unwrap();
    //     assert_eq!(expected, actual);
    // }
}
