use super::message::IGetBalanceByAddressRequest;
use super::message::IGetUtxosByAddressesRequest;
use crate::model::GetBalanceByAddressRequest;
use crate::model::GetUtxosByAddressesRequest;
use js_sys::Array;
use kaspa_addresses::Address;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::wasm_bindgen_test;
use workflow_wasm::extensions::ObjectExtension;

const TEST_ADDRESS_STR: &str = "kaspa:qpauqsvk7yf9unexwmxsnmg547mhyga37csh0kj53q6xxgl24ydxjsgzthw5j";

fn test_address() -> Address {
    Address::constructor(TEST_ADDRESS_STR)
}

#[wasm_bindgen_test]
fn test_get_balance_by_address_request_from_string() {
    let test_address = test_address();
    let args = IGetBalanceByAddressRequest::default();
    args.set("address", JsValue::from(TEST_ADDRESS_STR).as_ref()).unwrap();

    let request = GetBalanceByAddressRequest::try_from(args).unwrap();
    assert_eq!(request.address, test_address);
}

#[wasm_bindgen_test]
fn test_get_balance_by_address_request_from_address() {
    let test_address = test_address();
    let args = IGetBalanceByAddressRequest::default();
    args.set("address", JsValue::from(test_address.clone()).as_ref()).unwrap();

    let request = GetBalanceByAddressRequest::try_from(args).unwrap();
    assert_eq!(request.address, test_address);
}

#[wasm_bindgen_test]
fn test_get_utxos_by_addresses_request_from_address_array() {
    let test_address = test_address();
    let js_array = Array::new();
    js_array.push(&JsValue::from(test_address.clone()));
    let js_value = JsValue::from(js_array);

    let request = GetUtxosByAddressesRequest::try_from(IGetUtxosByAddressesRequest::from(js_value)).unwrap();
    assert_eq!(request.addresses, vec![test_address]);
}

#[wasm_bindgen_test]
fn test_get_utxos_by_addresses_request_from_object_with_address_array() {
    let test_address = test_address();
    let js_array = Array::new();
    js_array.push(&JsValue::from(test_address.clone()));

    let args = IGetUtxosByAddressesRequest::default();
    args.set("addresses", js_array.as_ref()).unwrap();

    let request = GetUtxosByAddressesRequest::try_from(args).unwrap();
    assert_eq!(request.addresses, vec![test_address]);
}
