use super::*;
use js_sys::Reflect;
use kaspa_addresses::{Prefix, Version};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn accepting_block_selector_accepts_hash_or_bigint() {
    let hash = RpcHash::from_bytes([0xab; 32]);
    for selector in [RpcAcceptingBlock::Hash(hash), RpcAcceptingBlock::DaaScore(u64::MAX)] {
        let args = Object::new();
        args.set("acceptingBlock", &to_value(&selector).unwrap()).unwrap();
        args.set("transactionIds", &to_value(&vec![hash]).unwrap()).unwrap();
        let request =
            GetTransactionsByAcceptingBlockRequest::try_from(IGetTransactionsByAcceptingBlockRequest::from(JsValue::from(args)))
                .unwrap();
        assert_eq!(request.accepting_block, selector);
        assert_eq!(request.transaction_ids, vec![hash]);
    }
    let selector = Object::new();
    selector.set("hash", &JsValue::from_str(&hash.to_string())).unwrap();
    selector.set("daaScore", &js_sys::BigInt::from(1u64)).unwrap();
    let args = Object::new();
    args.set("acceptingBlock", &selector).unwrap();
    args.set("transactionIds", &to_value(&vec![hash]).unwrap()).unwrap();
    assert!(
        GetTransactionsByAcceptingBlockRequest::try_from(IGetTransactionsByAcceptingBlockRequest::from(JsValue::from(args)),).is_err()
    );
}

#[wasm_bindgen_test]
fn accepting_block_response_preserves_populated_input() {
    use kaspa_consensus_core::{
        subnets::SUBNETWORK_ID_NATIVE,
        tx::{SignableTransaction, Transaction, TransactionInput, TransactionOutpoint, UtxoEntry},
    };
    let hash = RpcHash::from_bytes([0xab; 32]);
    let script = RpcScriptPublicKey::from_vec(0, vec![0x51]);
    let entry = UtxoEntry::new(42, script, u64::MAX, false, Some(hash));
    let transaction = SignableTransaction::with_entries(
        Transaction::new(
            1,
            vec![TransactionInput::new_with_compute_budget(TransactionOutpoint::new(hash, 0), vec![1, 2], 0, 123)],
            vec![],
            0,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        ),
        vec![entry],
    );
    let response = IGetTransactionsByAcceptingBlockResponse::try_from(GetTransactionsByAcceptingBlockResponse {
        transactions: vec![RpcOptionalTransaction::from(&transaction)],
    })
    .unwrap();
    let response = JsValue::from(response);
    let transactions = Array::from(&Reflect::get(&response, &"transactions".into()).unwrap());
    let input = Array::from(&Reflect::get(&transactions.get(0), &"inputs".into()).unwrap()).get(0);
    let verbose = Reflect::get(&input, &"verboseData".into()).unwrap();
    let entry = Reflect::get(&verbose, &"utxoEntry".into()).unwrap();
    assert_eq!(Reflect::get(&entry, &"amount".into()).unwrap(), JsValue::from(js_sys::BigInt::from(42u64)));
    assert_eq!(Reflect::get(&entry, &"blockDaaScore".into()).unwrap(), JsValue::from(js_sys::BigInt::from(u64::MAX)));
    assert_eq!(Reflect::get(&entry, &"scriptPublicKey".into()).unwrap().as_string().as_deref(), Some("000051"));
    assert_eq!(Reflect::get(&entry, &"covenantId".into()).unwrap().as_string(), Some(hash.to_string()));
    assert_eq!(Reflect::get(&input, &"computeBudget".into()).unwrap().as_f64(), Some(123.0));
}

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

#[wasm_bindgen_test]
fn get_utxos_by_covenant_ids_request_defaults_filters() {
    let covenant_id = RpcHash::from_bytes([1; 32]);
    let args = Object::new();
    args.set("covenantIds", &to_value(&vec![covenant_id]).unwrap()).unwrap();
    let request = GetUtxosByCovenantIdsRequest::try_from(IGetUtxosByCovenantIdsRequest::from(JsValue::from(args.clone()))).unwrap();
    assert_eq!(request.covenant_ids, vec![covenant_id]);
    assert!(request.script_public_keys.is_empty());
    assert!(request.cursor.is_none());
    assert!(request.budget.is_none());
}

#[wasm_bindgen_test]
fn get_utxos_by_covenant_ids_request_accepts_script_forms_and_bounds() {
    let covenant_id = RpcHash::from_bytes([1; 32]);
    let script_public_key = RpcScriptPublicKey::from_vec(0, vec![0x51]);
    let args = Object::new();
    args.set("covenantIds", &to_value(&vec![covenant_id]).unwrap()).unwrap();
    let script_object = Object::new();
    script_object.set("version", &JsValue::from(0)).unwrap();
    script_object.set("script", &JsValue::from_str("51")).unwrap();
    let scripts =
        Array::of3(&JsValue::from(script_public_key.clone()), &JsValue::from(script_object), &to_value(&script_public_key).unwrap());
    args.set("scriptPublicKeys", scripts.as_ref()).unwrap();
    args.set("fromDaaScore", &js_sys::BigInt::from(5u64)).unwrap();
    args.set("toDaaScore", &js_sys::BigInt::from(10u64)).unwrap();
    args.set("budget", &js_sys::BigInt::from(1000u64)).unwrap();
    let request = GetUtxosByCovenantIdsRequest::try_from(IGetUtxosByCovenantIdsRequest::from(JsValue::from(args.clone()))).unwrap();
    assert_eq!(request.covenant_ids, vec![covenant_id]);
    assert_eq!(request.script_public_keys, vec![script_public_key; 3]);
    assert_eq!(request.from_daa_score, Some(5));
    assert_eq!(request.to_daa_score, Some(10));
    assert_eq!(request.budget, Some(1000));
    assert_eq!(args.get_value("scriptPublicKeys").unwrap(), JsValue::from(scripts));
}

#[wasm_bindgen_test]
fn get_utxos_by_covenant_ids_response_preserves_entries_and_roundtrips_cursor() {
    let covenant_id = RpcHash::from_bytes([1; 32]);
    let script_public_key = RpcScriptPublicKey::from_vec(0, vec![0x51]);
    let outpoint = RpcTransactionOutpoint::from(cctx::TransactionOutpoint::new(RpcHash::from_bytes([2; 32]), 3));
    let entry = RpcUtxosByCovenantIdsEntry {
        address: None,
        outpoint,
        utxo_entry: RpcUtxoEntry::new(42, script_public_key.clone(), 7, true, Some(covenant_id)),
    };
    let cursor = RpcGetUtxosByCovenantIdsCursor::new(covenant_id, 7, script_public_key.clone(), outpoint);
    let response =
        IGetUtxosByCovenantIdsResponse::try_from(GetUtxosByCovenantIdsResponse::new(vec![entry], Some(cursor.clone()))).unwrap();
    let value = JsValue::from(response);
    let entries = Array::from(&Reflect::get(&value, &JsValue::from_str("entries")).unwrap());
    assert_eq!(entries.length(), 1);
    let js_entry = entries.get(0);
    let entry = UtxoEntryReference::try_ref_from_js_value(&js_entry).unwrap();
    assert_eq!(entry.utxo.address, None);
    assert_eq!(entry.utxo.outpoint.transaction_id(), outpoint.transaction_id);
    assert_eq!(entry.utxo.outpoint.index(), outpoint.index);
    assert_eq!(entry.utxo.amount, 42);
    assert_eq!(entry.utxo.script_public_key, script_public_key);
    assert_eq!(entry.utxo.block_daa_score, 7);
    assert!(entry.utxo.is_coinbase);
    assert_eq!(entry.utxo.covenant_id, Some(covenant_id));

    let js_cursor = Reflect::get(&value, &JsValue::from_str("nextCursor")).unwrap();
    let keys = Object::keys(&Object::from(js_cursor.clone())).join(",").as_string().unwrap();
    assert_eq!(keys, "startCovenantId,startDaaScore,startScriptPublicKey,startOutpoint");
    assert!(Reflect::get(&js_cursor, &JsValue::from_str("startDaaScore")).unwrap().is_bigint());
    assert!(Reflect::get(&js_cursor, &JsValue::from_str("startScriptPublicKey")).unwrap().is_string());
    let args = Object::new();
    args.set("covenantIds", &to_value(&vec![covenant_id]).unwrap()).unwrap();
    args.set("cursor", &js_cursor).unwrap();
    let request = GetUtxosByCovenantIdsRequest::try_from(IGetUtxosByCovenantIdsRequest::from(JsValue::from(args))).unwrap();
    assert_eq!(request.cursor, Some(cursor));
}

#[wasm_bindgen_test]
fn get_utxos_by_covenant_ids_empty_page_can_continue_or_finish() {
    let cursor = RpcGetUtxosByCovenantIdsCursor::new(
        RpcHash::from_bytes([1; 32]),
        7,
        RpcScriptPublicKey::from_vec(0, vec![0x51]),
        cctx::TransactionOutpoint::EMPTY.into(),
    );
    for cursor in [Some(cursor), None] {
        let has_cursor = cursor.is_some();
        let response = IGetUtxosByCovenantIdsResponse::try_from(GetUtxosByCovenantIdsResponse::new(vec![], cursor)).unwrap();
        let value = JsValue::from(response);
        assert_eq!(Array::from(&Reflect::get(&value, &JsValue::from_str("entries")).unwrap()).length(), 0);
        assert_eq!(Reflect::has(&value, &JsValue::from_str("nextCursor")).unwrap(), has_cursor);
    }
}

#[wasm_bindgen_test]
fn get_utxos_by_covenant_ids_request_rejects_invalid_ids_scripts_and_partial_cursors() {
    let args = Array::of1(&JsValue::from_str(&RpcHash::from_bytes([1; 32]).to_string()));
    assert!(GetUtxosByCovenantIdsRequest::try_from(IGetUtxosByCovenantIdsRequest::from(JsValue::from(args))).is_err());

    let args = Object::new();
    args.set("covenantIds", &Array::of1(&JsValue::from_str("invalid"))).unwrap();
    assert!(GetUtxosByCovenantIdsRequest::try_from(IGetUtxosByCovenantIdsRequest::from(JsValue::from(args.clone()))).is_err());
    args.set("covenantIds", &to_value(&vec![RpcHash::from_bytes([1; 32])]).unwrap()).unwrap();
    args.set("scriptPublicKeys", &JsValue::from_str("not an array")).unwrap();
    assert!(GetUtxosByCovenantIdsRequest::try_from(IGetUtxosByCovenantIdsRequest::from(JsValue::from(args.clone()))).is_err());
    args.set("scriptPublicKeys", &Array::of1(&JsValue::from_str("invalid"))).unwrap();
    assert!(GetUtxosByCovenantIdsRequest::try_from(IGetUtxosByCovenantIdsRequest::from(JsValue::from(args.clone()))).is_err());

    args.set("scriptPublicKeys", &Array::new()).unwrap();
    let cursor = Object::new();
    cursor.set("startCovenantId", &to_value(&RpcHash::from_bytes([1; 32])).unwrap()).unwrap();
    cursor.set("startDaaScore", &js_sys::BigInt::from(7u64)).unwrap();
    args.set("cursor", &cursor).unwrap();
    assert!(GetUtxosByCovenantIdsRequest::try_from(IGetUtxosByCovenantIdsRequest::from(JsValue::from(args))).is_err());
}
