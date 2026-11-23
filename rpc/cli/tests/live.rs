//! Live integration tests for `kaspa-rpc-cli` against a real Kaspa node.
//!
//! These tests are ENV-GATED: each one reads its node endpoint from an
//! environment variable and, when that variable is unset, prints a skip note
//! and returns early (passing). No node address is ever embedded in source, so
//! a CI run with no endpoint configured still passes (everything is skipped).
//!
//! Endpoints:
//!   - `KASPA_RPC_TEST_WRPC_URL`  e.g. `ws://<HOST>:17210`
//!   - `KASPA_RPC_TEST_GRPC_URL`  e.g. `grpc://<HOST>:16210`
//!
//! Run against a live node, for example:
//! ```text
//! KASPA_RPC_TEST_WRPC_URL=ws://<HOST>:17210 \
//!     cargo test -p kaspa-rpc-cli --test live -- --nocapture
//! ```
//! Both endpoints, serially:
//! ```text
//! KASPA_RPC_TEST_WRPC_URL=ws://<HOST>:17210 \
//! KASPA_RPC_TEST_GRPC_URL=grpc://<HOST>:16210 \
//!     cargo test -p kaspa-rpc-cli --test live -- --nocapture --test-threads=1
//! ```

use std::str::FromStr;
use std::time::Duration;

use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_notify::scope::{Scope, VirtualDaaScoreChangedScope};
use kaspa_rpc_cli::commands::call::Call;
use kaspa_rpc_cli::commands::get_block::GetBlock;
use kaspa_rpc_cli::commands::get_block_dag_info::GetBlockDagInfo;
use kaspa_rpc_cli::commands::get_coin_supply::GetCoinSupply;
use kaspa_rpc_cli::commands::get_info::GetInfo;
use kaspa_rpc_cli::{ConnectOptions, RpcCommand, connect};
use kaspa_rpc_core::Notification;
use kaspa_rpc_core::RpcHash;
use kaspa_rpc_core::notify::connection::{ChannelConnection, ChannelType};
use kaspa_wrpc_client::WrpcEncoding;

/// Read the named endpoint var; if it is unset or empty, print a skip note and
/// return from the calling test (a skipped test still counts as a pass).
macro_rules! endpoint_or_skip {
    ($var:literal) => {
        match std::env::var($var) {
            Ok(v) if !v.trim().is_empty() => v,
            _ => {
                println!("skipping: {} not set", $var);
                return;
            }
        }
    };
}

/// Build connection options for an explicit URL. Transport and encoding are
/// inferred from the URL scheme (wRPC defaults to borsh); the network only
/// selects a default port, which the explicit `:port` in the URL overrides.
fn options_for(url: String) -> ConnectOptions {
    ConnectOptions {
        url: Some(url),
        network: NetworkId::new(NetworkType::Mainnet),
        transport: None,
        encoding: WrpcEncoding::Borsh,
        timeout_ms: 30_000,
    }
}

/// Serialize a typed command response to a JSON value for field assertions.
fn to_value<T: serde::Serialize>(response: T) -> serde_json::Value {
    serde_json::to_value(response).expect("response serializes to JSON")
}

/// Assert that a `get-info` response is a well-formed object.
fn assert_get_info(value: &serde_json::Value) {
    let obj = value.as_object().expect("get-info returns a JSON object");
    assert!(obj.contains_key("serverVersion"), "get-info missing serverVersion: {value}");
    assert!(obj.contains_key("isSynced"), "get-info missing isSynced: {value}");
    assert!(value["serverVersion"].is_string(), "serverVersion should be a string: {value}");
    assert!(value["isSynced"].is_boolean(), "isSynced should be a boolean: {value}");
}

#[tokio::test]
async fn get_info_wrpc() {
    let url = endpoint_or_skip!("KASPA_RPC_TEST_WRPC_URL");
    let client = connect(&options_for(url)).await.expect("wrpc connect");
    let value = to_value(GetInfo {}.run(&client).await.expect("get-info over wrpc"));
    assert_get_info(&value);
    println!("wrpc get-info: {value}");
}

#[tokio::test]
async fn get_info_grpc() {
    let url = endpoint_or_skip!("KASPA_RPC_TEST_GRPC_URL");
    let client = connect(&options_for(url)).await.expect("grpc connect");
    let value = to_value(GetInfo {}.run(&client).await.expect("get-info over grpc"));
    assert_get_info(&value);
    println!("grpc get-info: {value}");
}

#[tokio::test]
async fn block_dag_info_then_block() {
    let url = endpoint_or_skip!("KASPA_RPC_TEST_WRPC_URL");
    let client = connect(&options_for(url)).await.expect("wrpc connect");

    let dag = to_value(GetBlockDagInfo {}.run(&client).await.expect("get-block-dag-info"));
    let sink = dag["sink"].as_str().expect("dag info exposes a sink hash string").to_string();
    let hash = RpcHash::from_str(&sink).expect("sink is a valid hash");

    let block = to_value(GetBlock { hash, include_transactions: false }.run(&client).await.expect("get-block by sink hash"));
    let obj = block.as_object().expect("get-block returns an object");
    assert!(obj.contains_key("block"), "get-block missing block field: {block}");
    let header = &block["block"]["header"];
    assert_eq!(header["hash"].as_str(), Some(sink.as_str()), "returned block hash should equal the requested sink");
}

#[tokio::test]
async fn coin_supply_numeric_fields() {
    let url = endpoint_or_skip!("KASPA_RPC_TEST_WRPC_URL");
    let client = connect(&options_for(url)).await.expect("wrpc connect");

    let value = to_value(GetCoinSupply {}.run(&client).await.expect("get-coin-supply"));
    // u64 fields serialize as JSON numbers or numeric strings depending on encoding.
    for key in ["maxSompi", "circulatingSompi"] {
        let field = &value[key];
        assert!(field.is_number() || field.as_str().is_some_and(|s| s.parse::<u128>().is_ok()), "{key} not numeric: {value}");
    }
}

#[tokio::test]
async fn generic_call_matches_typed() {
    let url = endpoint_or_skip!("KASPA_RPC_TEST_WRPC_URL");
    let client = connect(&options_for(url)).await.expect("wrpc connect");

    let typed = to_value(GetBlockDagInfo {}.run(&client).await.expect("typed get-block-dag-info"));
    let generic = Call { method: "get-block-dag-info".to_string(), params: None }.run(&client).await.expect("generic call");

    let mut typed_keys: Vec<&String> = typed.as_object().expect("typed object").keys().collect();
    let mut generic_keys: Vec<&String> = generic.as_object().expect("generic object").keys().collect();
    typed_keys.sort();
    generic_keys.sort();
    assert_eq!(typed_keys, generic_keys, "generic call keys differ from typed command keys");
}

#[tokio::test]
async fn subscription_smoke() {
    let url = endpoint_or_skip!("KASPA_RPC_TEST_WRPC_URL");
    let client = connect(&options_for(url)).await.expect("wrpc connect");

    // Replicate the minimal register/start/recv/stop pattern from the
    // `subscribe` command, bounded by a timeout so it never hangs.
    let (sender, receiver) = async_channel::unbounded::<Notification>();
    let connection = ChannelConnection::new("kaspa-rpc-cli-live-test", sender, ChannelType::Persistent);
    let listener_id = client.register_new_listener(connection);
    let scope = Scope::VirtualDaaScoreChanged(VirtualDaaScoreChangedScope {});
    client.start_notify(listener_id, scope.clone()).await.expect("start_notify");

    let received = tokio::time::timeout(Duration::from_secs(15), receiver.recv()).await;

    // Clean up regardless of the outcome.
    let _ = client.stop_notify(listener_id, scope).await;
    let _ = client.unregister_listener(listener_id).await;

    let notification = received.expect("a notification within 15s").expect("notification channel open");
    println!("received notification: {notification}");
}
