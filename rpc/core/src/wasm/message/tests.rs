use super::*;
use js_sys::Reflect;
use kaspa_addresses::{Prefix, Version};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn get_utxos_by_addresses_v2_request_accepts_address_array() {
    let address = Address::new(Prefix::Testnet, Version::PubKey, &[0; 32]);
    let args = Array::of1(&JsValue::from_str(&address.to_string()));
    let request = GetUtxosByAddressesV2Request::try_from(IGetUtxosByAddressesV2Request::from(JsValue::from(args))).unwrap();
    assert_eq!(request.addresses, vec![address]);
}

#[wasm_bindgen_test]
fn get_utxos_by_addresses_v2_request_accepts_object_with_address_instances_and_bounds() {
    let address = Address::new(Prefix::Testnet, Version::PubKey, &[0; 32]);
    let args = Object::new();
    Reflect::set(&args, &JsValue::from_str("addresses"), &Array::of1(&JsValue::from(address.clone()))).unwrap();
    Reflect::set(&args, &JsValue::from_str("fromDaaScore"), &js_sys::BigInt::from(5u64)).unwrap();
    Reflect::set(&args, &JsValue::from_str("limit"), &js_sys::BigInt::from(2u64)).unwrap();
    let cursor = Object::new();
    Reflect::set(&cursor, &JsValue::from_str("startAddress"), &JsValue::from_str(&address.to_string())).unwrap();
    Reflect::set(&cursor, &JsValue::from_str("startDaaScore"), &js_sys::BigInt::from(5u64)).unwrap();
    Reflect::set(&args, &JsValue::from_str("cursor"), &cursor).unwrap();
    let request = GetUtxosByAddressesV2Request::try_from(IGetUtxosByAddressesV2Request::from(JsValue::from(args))).unwrap();
    assert_eq!(request.addresses, vec![address]);
    assert_eq!(request.from_daa_score, Some(5));
    assert_eq!(request.limit, Some(2));
    assert_eq!(request.cursor.unwrap().start_daa_score, 5);
}

#[wasm_bindgen_test]
fn get_utxos_by_addresses_v2_response_returns_wasm_utxo_entries_and_camel_case_cursor() {
    let address = Address::new(Prefix::Testnet, Version::PubKey, &[0; 32]);
    let entry = RpcUtxosByAddressesEntry {
        address: Some(address.clone()),
        outpoint: cctx::TransactionOutpoint::EMPTY.into(),
        utxo_entry: RpcUtxoEntry::new(1, kaspa_txscript::pay_to_address_script(&address), 2, false, None),
    };
    let cursor = RpcGetUtxosByAddressesCursor::new(Some(address), 2, None);
    let response = IGetUtxosByAddressesV2Response::try_from(GetUtxosByAddressesV2Response::new(vec![entry], Some(cursor))).unwrap();
    let value = JsValue::from(response);
    let entries = Array::from(&Reflect::get(&value, &JsValue::from_str("entries")).unwrap());
    assert!(UtxoEntryReference::try_ref_from_js_value(&entries.get(0)).is_ok());
    let cursor = Reflect::get(&value, &JsValue::from_str("nextCursor")).unwrap();
    let keys = js_sys::Object::keys(&js_sys::Object::from(cursor.clone())).join(",").as_string().unwrap();
    assert!(Reflect::has(&cursor, &JsValue::from_str("startDaaScore")).unwrap(), "{keys}");
}
