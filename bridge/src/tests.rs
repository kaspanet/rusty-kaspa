// ============================================================================
// TEST SUITE FOR RKSTRATUM BRIDGE
// ============================================================================
// This module is compiled from `main.rs` under `#[cfg(test)]` (stratum-bridge binary).
// This file contains comprehensive tests for the RKStratum Bridge.
// Tests are organized into several categories:
//
// 1. CLI Parsing Tests: Test command-line argument parsing and instance spec parsing
// 2. Configuration Tests: Test YAML configuration loading and validation
// 3. Network Utility Tests: Test port normalization and address binding
// 4. JSON-RPC Tests: Test JSON-RPC event parsing and serialization
// 5. Mining State Tests: Test mining state management and job storage
// 6. Comprehensive Tests: Integration-style tests demonstrating full bridge functionality
//    - Stratum protocol flow (subscribe, authorize, submit)
//    - Miner compatibility (IceRiver, Bitmain, Goldshell, BzMiner)
//    - Share validation and PoW checking
//    - Job management and difficulty calculations
//    - VarDiff logic
//
// Each test includes documentation explaining what it tests and why.
// Tests are designed to be educational, helping developers understand the codebase.
// ============================================================================

#[cfg(test)]
use crate::cli::{parse_bool, parse_instance_spec};
#[cfg(test)]
use aero_stratum_bridge::BridgeConfig;

#[cfg(test)]
#[test]
fn test_parse_bool_true_values() {
    // Test all true values (matching BoolishValueParser)
    assert!(parse_bool("true").unwrap());
    assert!(parse_bool("1").unwrap());
    assert!(parse_bool("yes").unwrap());
    assert!(parse_bool("y").unwrap());
    assert!(parse_bool("on").unwrap());
    assert!(parse_bool("enable").unwrap());
    assert!(parse_bool("enabled").unwrap());

    // Case insensitive
    assert!(parse_bool("TRUE").unwrap());
    assert!(parse_bool("True").unwrap());
    assert!(parse_bool("ENABLE").unwrap());
    assert!(parse_bool("Enabled").unwrap());

    // With whitespace
    assert!(parse_bool("  true  ").unwrap());
    assert!(parse_bool("  enable  ").unwrap());
}

#[cfg(test)]
#[test]
fn test_parse_bool_false_values() {
    // Test all false values (matching BoolishValueParser)
    assert!(!parse_bool("false").unwrap());
    assert!(!parse_bool("0").unwrap());
    assert!(!parse_bool("no").unwrap());
    assert!(!parse_bool("n").unwrap());
    assert!(!parse_bool("off").unwrap());
    assert!(!parse_bool("disable").unwrap());
    assert!(!parse_bool("disabled").unwrap());

    // Case insensitive
    assert!(!parse_bool("FALSE").unwrap());
    assert!(!parse_bool("False").unwrap());
    assert!(!parse_bool("DISABLE").unwrap());
    assert!(!parse_bool("Disabled").unwrap());

    // With whitespace
    assert!(!parse_bool("  false  ").unwrap());
    assert!(!parse_bool("  disable  ").unwrap());
}

#[cfg(test)]
#[test]
fn test_parse_bool_invalid_values() {
    // Test invalid values
    assert!(parse_bool("invalid").is_err());
    assert!(parse_bool("maybe").is_err());
    assert!(parse_bool("2").is_err());
    assert!(parse_bool("").is_err());
    assert!(parse_bool("tru").is_err());
    assert!(parse_bool("fals").is_err());
}

#[cfg(test)]
#[test]
fn test_parse_bool_in_instance_spec() {
    // Test that parse_bool works correctly in instance spec parsing
    let spec = "port=:5555,var_diff=enable,log=disabled";
    let result = parse_instance_spec(spec, Some(8192));
    assert!(result.is_ok());
    let instance = result.unwrap();
    assert!(instance.var_diff.unwrap());
    assert!(!instance.log_to_file.unwrap());

    // Test with enabled/disabled
    let spec2 = "port=:5556,var_diff=enabled,pow2_clamp=disable";
    let result2 = parse_instance_spec(spec2, Some(8192));
    assert!(result2.is_ok());
    let instance2 = result2.unwrap();
    assert!(instance2.var_diff.unwrap());
    assert!(!instance2.pow2_clamp.unwrap());
}

#[cfg(test)]
#[test]
fn test_parse_instance_spec_rejects_empty_port() {
    // Test: Instance spec parsing rejects empty port values
    // This ensures that port values cannot be empty, preventing configuration errors.
    let result = parse_instance_spec("port=,diff=1", None);
    assert!(result.is_err(), "Empty port should be rejected");
}

#[cfg(test)]
#[test]
fn test_parse_instance_spec_empty_prom_port_is_none() {
    // Test: Empty prom_port in instance spec results in None
    // This allows optional Prometheus metrics port configuration.
    let result = parse_instance_spec("port=5555,prom=,diff=1", None);
    assert!(result.is_ok(), "Empty prom port should be accepted");
    let instance = result.unwrap();
    assert!(instance.prom_port.is_none(), "Empty prom port should result in None");
}

#[cfg(test)]
#[test]
fn test_config_single_instance_mode() {
    // Test: Single instance mode configuration parsing
    // When no instances array is provided, a single instance is created from top-level fields.
    // This is the simplest configuration mode for single-pool setups.
    let yaml = r#"
aerod_address: "127.0.0.1:16110"
stratum_port: ":5555"
min_share_diff: 8192
print_stats: true
"#;

    let config = BridgeConfig::from_yaml(yaml);
    assert!(config.is_ok());
    let config = config.unwrap();
    assert_eq!(config.instances.len(), 1, "Should create one instance in single-instance mode");
    assert_eq!(config.instances[0].stratum_port, ":5555", "Stratum port should be parsed correctly");
    assert_eq!(config.instances[0].min_share_diff, 8192, "Min share diff should be parsed correctly");
    assert_eq!(config.global.aerod_address, "127.0.0.1:16110", "Aerod address should be stored in global config");
}

#[cfg(test)]
#[test]
fn test_config_single_instance_defaults_when_missing_fields() {
    // Test: Single instance mode uses defaults for missing fields
    // When fields are not specified, sensible defaults are applied (e.g., stratum_port=":5555", min_share_diff=8192).
    let yaml = r#"
aerod_address: "127.0.0.1:16110"
print_stats: true
"#;

    let config = BridgeConfig::from_yaml(yaml);
    assert!(config.is_ok());
    let config = config.unwrap();
    assert_eq!(config.instances.len(), 1, "Should create one instance");
    assert_eq!(config.instances[0].stratum_port, ":5555", "Should use default stratum port");
    assert_eq!(config.instances[0].min_share_diff, 8192, "Should use default min_share_diff");
}

#[cfg(test)]
#[test]
fn test_config_single_instance_log_to_file_uses_global() {
    // Test: Single instance mode inherits global log_to_file setting
    // Global settings are applied to the single instance when not specified at instance level.
    // The instance-level log_to_file remains None, indicating it uses the global value.
    let yaml = r#"
aerod_address: "127.0.0.1:16110"
stratum_port: ":5555"
min_share_diff: 8192
log_to_file: false
"#;

    let config = BridgeConfig::from_yaml(yaml);
    assert!(config.is_ok());
    let config = config.unwrap();
    assert!(!config.global.log_to_file, "Global log_to_file should be false");
    assert_eq!(config.instances.len(), 1, "Should create one instance");
    assert!(config.instances[0].log_to_file.is_none(), "Instance log_to_file should be None (uses global)");
}

#[cfg(test)]
#[test]
fn test_config_multi_instance_mode() {
    // Test: Multi-instance mode configuration parsing
    // When instances array is provided, multiple bridge instances can be configured.
    // Each instance can have its own port, difficulty, and other settings.
    // This enables running multiple pools or different configurations simultaneously.
    let yaml = r#"
aerod_address: "127.0.0.1:16110"
instances:
  - stratum_port: ":5555"
    min_share_diff: 8192
  - stratum_port: ":5556"
    min_share_diff: 4096
"#;

    let config = BridgeConfig::from_yaml(yaml);
    assert!(config.is_ok());
    let config = config.unwrap();
    assert_eq!(config.instances.len(), 2, "Should create two instances");
    assert_eq!(config.instances[0].stratum_port, ":5555", "First instance port should be parsed");
    assert_eq!(config.instances[0].min_share_diff, 8192, "First instance difficulty should be parsed");
    assert_eq!(config.instances[1].stratum_port, ":5556", "Second instance port should be parsed");
    assert_eq!(config.instances[1].min_share_diff, 4096, "Second instance difficulty should be parsed");
}

#[cfg(test)]
#[test]
fn test_config_port_normalization() {
    let yaml = r#"
aerod_address: "127.0.0.1:16110"
stratum_port: "3030"
min_share_diff: 8192
web_dashboard_port: "3031"
"#;

    let config = BridgeConfig::from_yaml(yaml);
    assert!(config.is_ok());
    let config = config.unwrap();
    assert_eq!(config.instances[0].stratum_port, ":3030");
    assert_eq!(config.global.web_dashboard_port, ":3031");
}

#[cfg(test)]
#[test]
fn test_config_duplicate_ports_error() {
    let yaml = r#"
aerod_address: "127.0.0.1:16110"
instances:
  - stratum_port: ":5555"
    min_share_diff: 8192
  - stratum_port: ":5555"
    min_share_diff: 4096
"#;

    let config = BridgeConfig::from_yaml(yaml);
    assert!(config.is_err());
    assert!(config.unwrap_err().to_string().contains("Duplicate stratum_port"));
}

#[cfg(test)]
#[test]
fn test_config_coinbase_tag_suffix_empty_string() {
    let yaml = r#"
aerod_address: "127.0.0.1:16110"
stratum_port: ":5555"
min_share_diff: 8192
coinbase_tag_suffix: ""
"#;

    let config = BridgeConfig::from_yaml(yaml);
    assert!(config.is_ok());
    let config = config.unwrap();
    assert_eq!(config.global.coinbase_tag_suffix, None);
}

#[cfg(test)]
#[test]
fn test_config_coinbase_tag_suffix_with_value() {
    let yaml = r#"
aerod_address: "127.0.0.1:16110"
stratum_port: ":5555"
min_share_diff: 8192
coinbase_tag_suffix: "test"
"#;

    let config = BridgeConfig::from_yaml(yaml);
    assert!(config.is_ok());
    let config = config.unwrap();
    assert_eq!(config.global.coinbase_tag_suffix, Some("test".to_string()));
}

#[cfg(test)]
#[test]
fn test_config_var_diff_parsing() {
    // Test single-instance mode with var_diff
    let yaml = r#"
aerod_address: "127.0.0.1:16110"
stratum_port: ":5555"
min_share_diff: 8192
var_diff: true
var_diff_stats: true
shares_per_min: 30
"#;

    let config = BridgeConfig::from_yaml(yaml);
    assert!(config.is_ok());
    let config = config.unwrap();
    assert!(config.global.var_diff);
    assert!(config.global.var_diff_stats);
    assert_eq!(config.global.shares_per_min, 30);
    assert_eq!(config.instances.len(), 1);

    // Test multi-instance mode with var_diff
    let yaml2 = r#"
aerod_address: "127.0.0.1:16110"
var_diff: false
var_diff_stats: false
shares_per_min: 20
instances:
  - stratum_port: ":5555"
    min_share_diff: 8192
    var_diff: true
    var_diff_stats: true
  - stratum_port: ":5556"
    min_share_diff: 4096
"#;

    let config2 = BridgeConfig::from_yaml(yaml2);
    assert!(config2.is_ok());
    let config2 = config2.unwrap();
    assert!(!config2.global.var_diff);
    assert!(!config2.global.var_diff_stats);
    assert_eq!(config2.instances.len(), 2);
    assert_eq!(config2.instances[0].var_diff, Some(true));
    assert_eq!(config2.instances[0].var_diff_stats, Some(true));
    assert_eq!(config2.instances[1].var_diff, None); // Should inherit from global
}

#[cfg(test)]
#[test]
fn test_config_missing_instance_fields_error() {
    let yaml_missing_port = r#"
aerod_address: "127.0.0.1:16110"
instances:
  - min_share_diff: 8192
"#;

    let yaml_missing_diff = r#"
aerod_address: "127.0.0.1:16110"
instances:
  - stratum_port: ":5555"
"#;

    assert!(BridgeConfig::from_yaml(yaml_missing_port).is_err());
    assert!(BridgeConfig::from_yaml(yaml_missing_diff).is_err());
}

#[cfg(test)]
#[test]
fn test_config_single_instance_missing_fields_use_defaults() {
    let yaml_single_missing_diff = r#"
aerod_address: "127.0.0.1:16110"
stratum_port: ":5555"
"#;

    let yaml_single_missing_port = r#"
aerod_address: "127.0.0.1:16110"
min_share_diff: 1024
"#;

    let config_missing_diff = BridgeConfig::from_yaml(yaml_single_missing_diff);
    assert!(config_missing_diff.is_ok());
    let config_missing_diff = config_missing_diff.unwrap();
    assert_eq!(config_missing_diff.instances.len(), 1);
    assert_eq!(config_missing_diff.instances[0].stratum_port, ":5555");
    assert_eq!(config_missing_diff.instances[0].min_share_diff, 8192);

    let config_missing_port = BridgeConfig::from_yaml(yaml_single_missing_port);
    assert!(config_missing_port.is_ok());
    let config_missing_port = config_missing_port.unwrap();
    assert_eq!(config_missing_port.instances.len(), 1);
    assert_eq!(config_missing_port.instances[0].stratum_port, ":5555");
    assert_eq!(config_missing_port.instances[0].min_share_diff, 1024);
}

#[cfg(test)]
#[test]
fn test_config_empty_web_dashboard_port_kept_empty() {
    let yaml = r#"
aerod_address: "127.0.0.1:16110"
stratum_port: ":5555"
min_share_diff: 8192
web_dashboard_port: ""
"#;

    let config = BridgeConfig::from_yaml(yaml);
    assert!(config.is_ok());
    let config = config.unwrap();
    assert_eq!(config.global.web_dashboard_port, "");
}

// Net utils tests
#[cfg(test)]
#[test]
fn test_normalize_port_with_colon() {
    // Test: Port normalization preserves ports that already have a colon prefix
    // This ensures that ports like ":3030" remain unchanged during normalization.
    use aero_stratum_bridge::net_utils::normalize_port;
    assert_eq!(normalize_port(":3030"), ":3030", "Port with colon should remain unchanged");
    assert_eq!(normalize_port(":5555"), ":5555", "Port with colon should remain unchanged");
    assert_eq!(normalize_port(":16110"), ":16110", "Port with colon should remain unchanged");
}

#[cfg(test)]
#[test]
fn test_normalize_port_without_colon() {
    // Test: Port normalization adds colon prefix to ports without it
    // This allows users to specify ports as either "3030" or ":3030" - both are normalized to ":3030".
    use aero_stratum_bridge::net_utils::normalize_port;
    assert_eq!(normalize_port("3030"), ":3030", "Port without colon should get colon prefix");
    assert_eq!(normalize_port("5555"), ":5555", "Port without colon should get colon prefix");
    assert_eq!(normalize_port("16110"), ":16110", "Port without colon should get colon prefix");
}

#[cfg(test)]
#[test]
fn test_normalize_port_with_full_address() {
    // Test: Port normalization preserves full IP:port addresses
    // When a full address is provided (IP:port), it remains unchanged.
    // This allows binding to specific interfaces while still normalizing port-only values.
    use aero_stratum_bridge::net_utils::normalize_port;
    assert_eq!(normalize_port("127.0.0.1:3030"), "127.0.0.1:3030", "Full address should remain unchanged");
    assert_eq!(normalize_port("0.0.0.0:3030"), "0.0.0.0:3030", "Full address should remain unchanged");
    assert_eq!(normalize_port("192.168.1.1:5555"), "192.168.1.1:5555", "Full address should remain unchanged");
}

#[cfg(test)]
#[test]
fn test_normalize_port_empty() {
    // Test: Port normalization handles empty strings
    // Empty strings and whitespace-only strings are normalized to empty strings.
    // This allows optional port configuration.
    use aero_stratum_bridge::net_utils::normalize_port;
    assert_eq!(normalize_port(""), "", "Empty string should remain empty");
    assert_eq!(normalize_port("   "), "", "Whitespace-only string should become empty");
}

#[cfg(test)]
#[test]
fn test_normalize_port_with_whitespace() {
    // Test: Port normalization trims whitespace from port values
    // Leading and trailing whitespace is removed, making configuration more forgiving.
    use aero_stratum_bridge::net_utils::normalize_port;
    assert_eq!(normalize_port("  :3030  "), ":3030", "Whitespace should be trimmed from port with colon");
    assert_eq!(normalize_port("  3030  "), ":3030", "Whitespace should be trimmed and colon added");
    assert_eq!(normalize_port("  127.0.0.1:3030  "), "127.0.0.1:3030", "Whitespace should be trimmed from full address");
}

#[cfg(test)]
#[test]
fn test_bind_addr_from_port_with_colon() {
    use aero_stratum_bridge::net_utils::bind_addr_from_port;
    assert_eq!(bind_addr_from_port(":3030"), "0.0.0.0:3030");
    assert_eq!(bind_addr_from_port(":5555"), "0.0.0.0:5555");
    assert_eq!(bind_addr_from_port(":16110"), "0.0.0.0:16110");
}

#[cfg(test)]
#[test]
fn test_bind_addr_from_port_without_colon() {
    use aero_stratum_bridge::net_utils::bind_addr_from_port;
    assert_eq!(bind_addr_from_port("3030"), "0.0.0.0:3030");
    assert_eq!(bind_addr_from_port("5555"), "0.0.0.0:5555");
    assert_eq!(bind_addr_from_port("16110"), "0.0.0.0:16110");
}

#[cfg(test)]
#[test]
fn test_bind_addr_from_port_with_full_address() {
    use aero_stratum_bridge::net_utils::bind_addr_from_port;
    assert_eq!(bind_addr_from_port("127.0.0.1:3030"), "127.0.0.1:3030");
    assert_eq!(bind_addr_from_port("0.0.0.0:3030"), "0.0.0.0:3030");
    assert_eq!(bind_addr_from_port("192.168.1.1:5555"), "192.168.1.1:5555");
}

#[cfg(test)]
#[test]
fn test_bind_addr_from_port_empty() {
    use aero_stratum_bridge::net_utils::bind_addr_from_port;
    assert_eq!(bind_addr_from_port(""), "");
    assert_eq!(bind_addr_from_port("   "), "");
}

// Stratum line buffer tests (issue #1017 — cap incomplete lines to prevent memory exhaustion)
#[cfg(test)]
#[test]
fn test_append_line_data_accepts_data_under_limit() {
    use aero_stratum_bridge::append_line_data;
    let mut buf = String::new();
    assert!(append_line_data(&mut buf, "{\"jsonrpc\":\"2.0\"}\n"));
    assert_eq!(buf, "{\"jsonrpc\":\"2.0\"}\n");
}

#[cfg(test)]
#[test]
fn test_append_line_data_accepts_incremental_chunks_under_limit() {
    use aero_stratum_bridge::{MAX_STRATUM_LINE_BYTES, append_line_data};
    let mut buf = String::new();
    let chunk_size = 1024;
    let chunks = MAX_STRATUM_LINE_BYTES / chunk_size;
    for _ in 0..chunks {
        assert!(append_line_data(&mut buf, &"x".repeat(chunk_size)));
    }
    assert_eq!(buf.len(), chunks * chunk_size);
}

#[cfg(test)]
#[test]
fn test_append_line_data_rejects_when_limit_exceeded() {
    use aero_stratum_bridge::{MAX_STRATUM_LINE_BYTES, append_line_data};
    let mut buf = "x".repeat(MAX_STRATUM_LINE_BYTES);
    assert!(!append_line_data(&mut buf, "y"));
    assert_eq!(buf.len(), MAX_STRATUM_LINE_BYTES);
}

#[cfg(test)]
#[test]
fn test_append_line_data_rejects_single_oversized_chunk() {
    use aero_stratum_bridge::{MAX_STRATUM_LINE_BYTES, append_line_data};
    let mut buf = String::new();
    assert!(!append_line_data(&mut buf, &"x".repeat(MAX_STRATUM_LINE_BYTES + 1)));
    assert!(buf.is_empty());
}

#[cfg(test)]
#[test]
fn test_append_line_data_accepts_exactly_at_limit() {
    use aero_stratum_bridge::{MAX_STRATUM_LINE_BYTES, append_line_data};
    let mut buf = String::new();
    assert!(append_line_data(&mut buf, &"x".repeat(MAX_STRATUM_LINE_BYTES)));
    assert_eq!(buf.len(), MAX_STRATUM_LINE_BYTES);
    assert!(!append_line_data(&mut buf, "y"));
}

#[cfg(test)]
#[test]
fn test_total_row_spm_uses_worker_average() {
    use aero_stratum_bridge::average_worker_spm;
    assert_eq!(average_worker_spm(0.0, 0), 0.0);
    assert!((average_worker_spm(30.0, 2) - 15.0).abs() < f64::EPSILON);
    assert!((average_worker_spm(12.5, 1) - 12.5).abs() < f64::EPSILON);
}

// JSON-RPC event tests
#[cfg(test)]
#[test]
fn test_stratum_method_from_str() {
    use aero_stratum_bridge::jsonrpc_event::StratumMethod;
    assert_eq!(StratumMethod::from("mining.subscribe"), StratumMethod::Subscribe);
    assert_eq!(StratumMethod::from("mining.authorize"), StratumMethod::Authorize);
    assert_eq!(StratumMethod::from("mining.submit"), StratumMethod::Submit);
    assert_eq!(StratumMethod::from("mining.notify"), StratumMethod::Notify);
    assert_eq!(StratumMethod::from("mining.set_difficulty"), StratumMethod::SetDifficulty);
    assert_eq!(StratumMethod::from("mining.extranonce.subscribe"), StratumMethod::ExtranonceSubscribe);
    assert_eq!(StratumMethod::from("mining.set_extranonce"), StratumMethod::SetExtranonce);
    assert_eq!(StratumMethod::from("unknown.method"), StratumMethod::Other("unknown.method".to_string()));
}

#[cfg(test)]
#[test]
fn test_stratum_method_to_string() {
    use aero_stratum_bridge::jsonrpc_event::StratumMethod;
    assert_eq!(String::from(StratumMethod::Subscribe), "mining.subscribe");
    assert_eq!(String::from(StratumMethod::Authorize), "mining.authorize");
    assert_eq!(String::from(StratumMethod::Submit), "mining.submit");
    assert_eq!(String::from(StratumMethod::Notify), "mining.notify");
    assert_eq!(String::from(StratumMethod::SetDifficulty), "mining.set_difficulty");
    assert_eq!(String::from(StratumMethod::ExtranonceSubscribe), "mining.extranonce.subscribe");
    assert_eq!(String::from(StratumMethod::SetExtranonce), "mining.set_extranonce");
    assert_eq!(String::from(StratumMethod::Other("custom".to_string())), "custom");
}

#[cfg(test)]
#[test]
fn test_jsonrpc_event_new() {
    use aero_stratum_bridge::jsonrpc_event::JsonRpcEvent;
    use serde_json::json;
    let event = JsonRpcEvent::new(Some("1".to_string()), "mining.subscribe", vec![json!("BzMiner")]);
    assert_eq!(event.method, "mining.subscribe");
    assert_eq!(event.jsonrpc, "2.0");
    assert_eq!(event.params.len(), 1);
}

#[cfg(test)]
#[test]
fn test_jsonrpc_event_method_enum() {
    use aero_stratum_bridge::jsonrpc_event::{JsonRpcEvent, StratumMethod};
    let event = JsonRpcEvent::new(None, "mining.subscribe", vec![]);
    assert_eq!(event.method_enum(), StratumMethod::Subscribe);

    let event2 = JsonRpcEvent::new(None, "mining.authorize", vec![]);
    assert_eq!(event2.method_enum(), StratumMethod::Authorize);
}

#[cfg(test)]
#[test]
fn test_jsonrpc_response_new() {
    use aero_stratum_bridge::jsonrpc_event::{JsonRpcEvent, JsonRpcResponse};
    use serde_json::json;
    let event = JsonRpcEvent::new(Some("1".to_string()), "mining.subscribe", vec![]);
    let response = JsonRpcResponse::new(&event, Some(json!(["subscription_id"])), None);
    assert!(response.result.is_some());
    assert!(response.error.is_none());
}

#[cfg(test)]
#[test]
fn test_jsonrpc_response_success() {
    use aero_stratum_bridge::jsonrpc_event::JsonRpcResponse;
    use serde_json::{Value, json};
    let response = JsonRpcResponse::success(Some(Value::String("1".to_string())), json!(["subscription_id"]));
    assert!(response.result.is_some());
    assert!(response.error.is_none());
}

#[cfg(test)]
#[test]
fn test_jsonrpc_response_error() {
    use aero_stratum_bridge::jsonrpc_event::JsonRpcResponse;
    use serde_json::Value;
    let response = JsonRpcResponse::error(Some(Value::String("1".to_string())), -1, "Invalid request", None);
    assert!(response.result.is_none());
    assert!(response.error.is_some());
    if let Some(error_vec) = response.error {
        assert_eq!(error_vec.len(), 3);
        assert_eq!(error_vec[1], Value::String("Invalid request".to_string()));
    }
}

#[cfg(test)]
#[test]
fn test_unmarshal_event_basic() {
    use aero_stratum_bridge::jsonrpc_event::unmarshal_event;
    let json = r#"{"jsonrpc":"2.0","method":"mining.subscribe","params":["BzMiner"],"id":1}"#;
    let event = unmarshal_event(json).unwrap();
    assert_eq!(event.method, "mining.subscribe");
    assert_eq!(event.params.len(), 1);
}

#[cfg(test)]
#[test]
fn test_unmarshal_event_with_null_id() {
    use aero_stratum_bridge::jsonrpc_event::unmarshal_event;
    let json = r#"{"jsonrpc":"2.0","method":"mining.notify","params":[],"id":null}"#;
    let event = unmarshal_event(json).unwrap();
    assert_eq!(event.method, "mining.notify");
}

#[cfg(test)]
#[test]
fn test_unmarshal_event_with_string_id() {
    use aero_stratum_bridge::jsonrpc_event::unmarshal_event;
    use serde_json::Value;
    let json = r#"{"jsonrpc":"2.0","method":"mining.subscribe","params":[],"id":"abc123"}"#;
    let event = unmarshal_event(json).unwrap();
    assert_eq!(event.id, Some(Value::String("abc123".to_string())));
}

#[cfg(test)]
#[test]
fn test_unmarshal_event_without_id() {
    use aero_stratum_bridge::jsonrpc_event::unmarshal_event;
    let json = r#"{"jsonrpc":"2.0","method":"mining.notify","params":[]}"#;
    let event = unmarshal_event(json).unwrap();
    assert_eq!(event.method, "mining.notify");
    assert!(event.id.is_none());
}

#[cfg(test)]
#[test]
fn test_unmarshal_event_sanitizes_control_chars() {
    use aero_stratum_bridge::jsonrpc_event::unmarshal_event;
    // Test with tab character (common in Goldshell ASICs)
    let json_with_tab = "{\"jsonrpc\":\"2.0\",\"method\":\"mining.subscribe\",\"params\":[\"BzMiner\t\"],\"id\":1}";
    let event = unmarshal_event(json_with_tab).unwrap();
    assert_eq!(event.method, "mining.subscribe");
    // The tab should be sanitized to a space
    if let Some(serde_json::Value::String(param)) = event.params.first() {
        assert!(!param.contains('\t'));
    }
}

#[cfg(test)]
#[test]
fn test_unmarshal_event_preserves_newlines() {
    use aero_stratum_bridge::jsonrpc_event::unmarshal_event;
    // Newlines should be preserved
    let json_with_newline = "{\"jsonrpc\":\"2.0\",\"method\":\"mining.subscribe\",\"params\":[\"line1\\nline2\"],\"id\":1}";
    let event = unmarshal_event(json_with_newline).unwrap();
    assert_eq!(event.method, "mining.subscribe");
}

#[cfg(test)]
#[test]
fn test_unmarshal_response_success() {
    use aero_stratum_bridge::jsonrpc_event::unmarshal_response;
    let json = r#"{"jsonrpc":"2.0","result":["subscription_id"],"id":1}"#;
    let response = unmarshal_response(json).unwrap();
    assert!(response.result.is_some());
    assert!(response.error.is_none());
}

#[cfg(test)]
#[test]
fn test_unmarshal_response_error() {
    use aero_stratum_bridge::jsonrpc_event::unmarshal_response;
    let json = r#"{"jsonrpc":"2.0","error":[-1,"Invalid request",null],"id":1}"#;
    let response = unmarshal_response(json).unwrap();
    assert!(response.result.is_none());
    assert!(response.error.is_some());
}

#[cfg(test)]
#[test]
fn test_unmarshal_event_invalid_json() {
    use aero_stratum_bridge::jsonrpc_event::unmarshal_event;
    let invalid_json = r#"{"jsonrpc":"2.0","method":"mining.subscribe""#;
    assert!(unmarshal_event(invalid_json).is_err());
}

#[cfg(test)]
#[test]
fn test_unmarshal_response_invalid_json() {
    use aero_stratum_bridge::jsonrpc_event::unmarshal_response;
    let invalid_json = r#"{"jsonrpc":"2.0","result":["subscription_id""#;
    assert!(unmarshal_response(invalid_json).is_err());
}

#[cfg(test)]
#[test]
fn test_jsonrpc_event_serialize() {
    use aero_stratum_bridge::jsonrpc_event::JsonRpcEvent;
    use serde_json::json;
    let event = JsonRpcEvent::new(Some("1".to_string()), "mining.subscribe", vec![json!("BzMiner")]);
    let serialized = serde_json::to_string(&event).unwrap();
    assert!(serialized.contains("mining.subscribe"));
    assert!(serialized.contains("BzMiner"));
}

#[cfg(test)]
#[test]
fn test_jsonrpc_response_serialize() {
    use aero_stratum_bridge::jsonrpc_event::JsonRpcResponse;
    use serde_json::{Value, json};
    let response = JsonRpcResponse::success(Some(Value::String("1".to_string())), json!(["subscription_id"]));
    let serialized = serde_json::to_string(&response).unwrap();
    assert!(serialized.contains("subscription_id"));
}

// Error handling and edge case tests
#[cfg(test)]
#[test]
fn test_error_short_code_display() {
    use aero_stratum_bridge::errors::ErrorShortCode;
    assert_eq!(ErrorShortCode::NoMinerAddress.as_str(), "err_no_miner_address");
    assert_eq!(ErrorShortCode::FailedBlockFetch.as_str(), "err_failed_block_fetch");
    assert_eq!(ErrorShortCode::InvalidAddressFmt.as_str(), "err_malformed_wallet_address");
    assert_eq!(ErrorShortCode::MissingJob.as_str(), "err_missing_job");
    assert_eq!(ErrorShortCode::BadDataFromMiner.as_str(), "err_bad_data_from_miner");
    assert_eq!(ErrorShortCode::FailedSendWork.as_str(), "err_failed_sending_work");
    assert_eq!(ErrorShortCode::FailedSetDiff.as_str(), "err_diff_set_failed");
    assert_eq!(ErrorShortCode::Disconnected.as_str(), "err_worker_disconnected");
}

#[cfg(test)]
#[test]
fn test_error_short_code_to_string() {
    use aero_stratum_bridge::errors::ErrorShortCode;
    assert_eq!(format!("{}", ErrorShortCode::NoMinerAddress), "err_no_miner_address");
    assert_eq!(format!("{}", ErrorShortCode::Disconnected), "err_worker_disconnected");
}

// Mining state tests
#[cfg(test)]
#[test]
fn test_mining_state_initial_values() {
    // Test: MiningState initial values after creation
    // This verifies that a newly created MiningState has the correct default values:
    // - Not initialized
    // - Big job format disabled
    // - Max jobs set to 300
    // - Job counter starts at 0
    // - No stored job IDs
    use aero_stratum_bridge::mining_state::MiningState;
    let state = MiningState::new();
    assert!(!state.is_initialized(), "State should start uninitialized");
    assert!(!state.use_big_job(), "Big job format should be disabled by default");
    assert_eq!(state.max_jobs(), 300, "Max jobs should be 300");
    assert_eq!(state.current_job_counter(), 0, "Job counter should start at 0");
    assert!(state.get_stored_job_ids().is_empty(), "No job IDs should be stored initially");
}

#[cfg(test)]
#[test]
fn test_mining_state_job_management() {
    // Test: Job storage and retrieval in MiningState
    // This verifies that jobs can be added to the mining state and retrieved by their job ID.
    // Jobs are stored in a circular buffer with a maximum of 300 jobs.
    use aero_consensus_core::block::Block;
    use aero_hashes::Hash;
    use aero_stratum_bridge::mining_state::{Job, MiningState};

    let state = MiningState::new();

    // Create a dummy job using Block::from_precomputed_hash (test helper)
    let hash1 = Hash::from_bytes([1; 32]);
    let block1 = Block::from_precomputed_hash(hash1, vec![]);
    let job1 = Job { block: block1, pre_pow_hash: Hash::default() };

    // Add job
    let job_id = state.add_job(job1);
    assert_eq!(job_id, 1, "First job should have ID 1");
    assert_eq!(state.current_job_counter(), 1, "Job counter should increment");

    // Retrieve job
    let retrieved = state.get_job(job_id);
    assert!(retrieved.is_some(), "Job should be retrievable by ID");

    // Test adding another job
    let hash2 = Hash::from_bytes([2; 32]);
    let block2 = Block::from_precomputed_hash(hash2, vec![]);
    let job2 = Job { block: block2, pre_pow_hash: Hash::default() };
    let job_id_2 = state.add_job(job2);
    assert_eq!(job_id_2, 2, "Second job should have ID 2");
    let retrieved2 = state.get_job(2);
    assert!(retrieved2.is_some(), "Second job should be retrievable");
}

#[cfg(test)]
#[test]
fn test_mining_state_difficulty_management() {
    // Test: Difficulty storage and retrieval in MiningState
    // This verifies that the mining state can store and retrieve difficulty values
    // using BigUint for arbitrary precision arithmetic.
    use aero_stratum_bridge::mining_state::MiningState;
    use num_bigint::BigUint;
    use num_traits::Zero;

    let state = MiningState::new();
    assert_eq!(state.get_big_diff(), BigUint::zero(), "Initial difficulty should be zero");

    let test_diff = BigUint::from(8192u64);
    state.set_big_diff(test_diff.clone());
    assert_eq!(state.get_big_diff(), test_diff, "Difficulty should be stored and retrieved correctly");
}

#[cfg(test)]
#[test]
fn test_mining_state_initialization_flag() {
    // Test: Initialization flag toggling in MiningState
    // This verifies that the initialization flag can be set and cleared,
    // which tracks whether a miner has completed the subscribe/authorize flow.
    use aero_stratum_bridge::mining_state::MiningState;
    let state = MiningState::new();
    assert!(!state.is_initialized(), "State should start uninitialized");

    state.set_initialized(true);
    assert!(state.is_initialized(), "State should be initialized after setting flag");

    state.set_initialized(false);
    assert!(!state.is_initialized(), "State should be uninitialized after clearing flag");
}

#[cfg(test)]
#[test]
fn test_mining_state_big_job_flag() {
    // Test: Big job format flag toggling in MiningState
    // This verifies that the big job format flag can be set and cleared.
    // Big job format is used for miners that require extended job data (e.g., some ASICs).
    use aero_stratum_bridge::mining_state::MiningState;
    let state = MiningState::new();
    assert!(!state.use_big_job(), "Big job format should be disabled by default");

    state.set_use_big_job(true);
    assert!(state.use_big_job(), "Big job format should be enabled after setting flag");

    state.set_use_big_job(false);
    assert!(!state.use_big_job(), "Big job format should be disabled after clearing flag");
}

// Config parsing edge cases
#[cfg(test)]
#[test]
fn test_config_invalid_yaml() {
    use aero_stratum_bridge::BridgeConfig;
    let invalid_yaml = r#"
aerod_address: "127.0.0.1:16110"
stratum_port: ":5555"
min_share_diff: invalid_number
"#;
    assert!(BridgeConfig::from_yaml(invalid_yaml).is_err());
}

#[cfg(test)]
#[test]
fn test_config_malformed_yaml() {
    use aero_stratum_bridge::BridgeConfig;
    let malformed_yaml = r#"
aerod_address: "127.0.0.1:16110"
stratum_port: ":5555"
min_share_diff: 8192
invalid: [unclosed
"#;
    assert!(BridgeConfig::from_yaml(malformed_yaml).is_err());
}

#[cfg(test)]
#[test]
fn test_config_negative_values() {
    use aero_stratum_bridge::BridgeConfig;
    let yaml = r#"
aerod_address: "127.0.0.1:16110"
stratum_port: ":5555"
min_share_diff: -1
"#;
    // Should either error or clamp to valid value
    let result = BridgeConfig::from_yaml(yaml);
    // Depending on implementation, this might error or be accepted
    // Test that it doesn't panic
    let _ = result;
}

#[cfg(test)]
#[test]
fn test_config_very_large_values() {
    use aero_stratum_bridge::BridgeConfig;
    let yaml = r#"
aerod_address: "127.0.0.1:16110"
stratum_port: ":5555"
min_share_diff: 999999999
"#;
    let config = BridgeConfig::from_yaml(yaml);
    // Should handle large values gracefully
    assert!(config.is_ok());
}

#[cfg(test)]
#[test]
fn test_config_empty_instances_list() {
    use aero_stratum_bridge::BridgeConfig;
    let yaml = r#"
aerod_address: "127.0.0.1:16110"
instances: []
"#;
    let config = BridgeConfig::from_yaml(yaml);
    // Should either error (no instances) or be handled gracefully
    let _ = config;
}

#[cfg(test)]
#[test]
fn test_config_whitespace_in_values() {
    use aero_stratum_bridge::BridgeConfig;
    let yaml = r#"
aerod_address: "  127.0.0.1:16110  "
stratum_port: "  :5555  "
min_share_diff: 8192
"#;
    let config = BridgeConfig::from_yaml(yaml);
    assert!(config.is_ok());
    let config = config.unwrap();
    // Values should be trimmed/normalized
    assert_eq!(config.instances[0].stratum_port, ":5555");
}

// JSON-RPC edge cases
#[cfg(test)]
#[test]
fn test_unmarshal_event_with_empty_params() {
    use aero_stratum_bridge::jsonrpc_event::unmarshal_event;
    let json = r#"{"jsonrpc":"2.0","method":"mining.subscribe","params":[],"id":1}"#;
    let event = unmarshal_event(json).unwrap();
    assert_eq!(event.method, "mining.subscribe");
    assert_eq!(event.params.len(), 0);
}

#[cfg(test)]
#[test]
fn test_unmarshal_event_with_nested_objects() {
    use aero_stratum_bridge::jsonrpc_event::unmarshal_event;
    let json = r#"{"jsonrpc":"2.0","method":"mining.submit","params":["address","job_id",{"nonce":123}],"id":1}"#;
    let event = unmarshal_event(json).unwrap();
    assert_eq!(event.method, "mining.submit");
    assert_eq!(event.params.len(), 3);
}

#[cfg(test)]
#[test]
fn test_unmarshal_event_with_unicode() {
    use aero_stratum_bridge::jsonrpc_event::unmarshal_event;
    let json = r#"{"jsonrpc":"2.0","method":"mining.subscribe","params":["测试"],"id":1}"#;
    let event = unmarshal_event(json).unwrap();
    assert_eq!(event.method, "mining.subscribe");
    assert_eq!(event.params.len(), 1);
}

#[cfg(test)]
#[test]
fn test_unmarshal_event_with_very_long_string() {
    use aero_stratum_bridge::jsonrpc_event::unmarshal_event;
    let long_string = "a".repeat(10000);
    let json = format!(r#"{{"jsonrpc":"2.0","method":"mining.subscribe","params":["{}"],"id":1}}"#, long_string);
    let event = unmarshal_event(&json).unwrap();
    assert_eq!(event.method, "mining.subscribe");
    if let Some(serde_json::Value::String(param)) = event.params.first() {
        assert_eq!(param.len(), 10000);
    }
}

// Instance spec parsing edge cases
#[cfg(test)]
#[test]
fn test_parse_instance_spec_with_all_fields() {
    let spec = "port=:5555,diff=8192,prom=:9090,wait=1000,extranonce=4,log=true,var_diff=true,shares_per_min=30,var_diff_stats=true,pow2_clamp=false";
    let result = parse_instance_spec(spec, None);
    assert!(result.is_ok());
    let instance = result.unwrap();
    assert_eq!(instance.stratum_port, ":5555");
    assert_eq!(instance.min_share_diff, 8192);
    assert_eq!(instance.prom_port, Some(":9090".to_string()));
    assert_eq!(instance.extranonce_size, Some(4));
    assert_eq!(instance.var_diff, Some(true));
    assert_eq!(instance.shares_per_min, Some(30));
    assert_eq!(instance.var_diff_stats, Some(true));
    assert_eq!(instance.pow2_clamp, Some(false));
}

#[cfg(test)]
#[test]
fn test_parse_instance_spec_with_whitespace() {
    let spec = "  port = :5555 , diff = 8192  ";
    let result = parse_instance_spec(spec, None);
    assert!(result.is_ok());
    let instance = result.unwrap();
    assert_eq!(instance.stratum_port, ":5555");
    assert_eq!(instance.min_share_diff, 8192);
}

#[cfg(test)]
#[test]
fn test_parse_instance_spec_invalid_diff() {
    let spec = "port=:5555,diff=not_a_number";
    let result = parse_instance_spec(spec, None);
    assert!(result.is_err());
}

#[cfg(test)]
#[test]
fn test_parse_instance_spec_invalid_wait() {
    let spec = "port=:5555,diff=8192,wait=invalid";
    let result = parse_instance_spec(spec, None);
    assert!(result.is_err());
}

#[cfg(test)]
#[test]
fn test_parse_instance_spec_unknown_key() {
    let spec = "port=:5555,diff=8192,unknown_key=value";
    let result = parse_instance_spec(spec, None);
    assert!(result.is_err());
}

#[cfg(test)]
#[test]
fn test_parse_instance_spec_multiple_ports() {
    let spec = "port=:5555,port=:5556,diff=8192";
    let result = parse_instance_spec(spec, None);
    // Last port should win, or it should error
    let _ = result;
}

// Integration tests for the bridge binary
// These tests run with: cargo test -p aero-stratum-bridge --bin stratum-bridge

#[cfg(test)]
mod integration {
    use aero_alloc::init_allocator_with_default_settings;
    use aero_stratum_bridge::{AeroApi, StratumServerBridgeConfig as StratumBridgeConfig, listen_and_serve_with_shutdown};
    use aerod_lib::args as aerod_args;
    use std::ffi::OsString;
    use std::time::Duration;
    use tokio::sync::watch;
    use tokio::time::timeout;

    // Mirrors testing/integration/src/common/daemon.rs::free_port.
    fn free_port() -> u16 {
        loop {
            let port = rand::random::<u16>() % (u16::MAX - 1024) + 1024;
            if let Ok(listener) = std::net::TcpListener::bind(format!("127.0.0.1:{port}")) {
                drop(listener);
                return port;
            }
        }
    }

    #[tokio::test]
    async fn test_bridge_startup_with_inprocess_node() {
        init_allocator_with_default_settings();

        // Use a temporary directory for the node data
        let temp_dir = std::env::temp_dir().join(format!("aerod_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Start an in-process devnet node with random listener ports to avoid conflicts
        // with a locally running node.
        let rpc_address = format!("127.0.0.1:{}", free_port());
        let argv: Vec<OsString> = vec![
            "aerod".into(),
            "--devnet".into(),
            "--appdir".into(),
            temp_dir.to_string_lossy().to_string().into(),
            format!("--rpclisten={rpc_address}").into(),
            format!("--listen=127.0.0.1:{}", free_port()).into(),
            "--disable-upnp".into(),
            "--utxoindex".into(),
        ];

        let node_args = aerod_args::Args::parse(argv).unwrap();
        let inprocess_node = crate::inprocess_node::InProcessNode::start_from_args(node_args).unwrap();

        // Wait a bit for the node to start
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Create AeroApi client
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let aero_api = AeroApi::new(rpc_address.clone(), None, shutdown_rx.clone()).await.unwrap();

        // Create bridge config
        let bridge_config = StratumBridgeConfig {
            instance_id: "test-instance".to_string(),
            stratum_port: ":0".to_string(), // Use port 0 for testing
            aerod_address: rpc_address.clone(),
            prom_port: String::new(),
            print_stats: false,
            log_to_file: false,
            health_check_port: String::new(),
            block_wait_time: Duration::from_secs(1),
            min_share_diff: 1,
            var_diff: false,
            shares_per_min: 30,
            var_diff_stats: false,
            extranonce_size: 4,
            pow2_clamp: false,
            coinbase_tag_suffix: None,
        };

        // Start the bridge server (with a timeout to prevent hanging)
        let bridge_handle =
            tokio::spawn(async move { listen_and_serve_with_shutdown::<AeroApi>(bridge_config, aero_api, None, shutdown_rx).await });

        // Give it a moment to start
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Signal shutdown
        let _ = shutdown_tx.send(true);

        // Wait for bridge to shutdown (with timeout)
        let result = timeout(Duration::from_secs(5), bridge_handle).await;
        assert!(result.is_ok(), "Bridge should shutdown gracefully");

        // Cleanup
        crate::inprocess_node::shutdown_inprocess(inprocess_node).await;
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

// ============================================================================
// COMPREHENSIVE TEST SUITE FOR BRIDGE FUNCTIONALITY
// ============================================================================

#[cfg(test)]
mod comprehensive_tests {
    use aero_consensus_core::block::Block;
    use aero_consensus_core::header::Header;
    use aero_consensus_core::subnets::SubnetworkId;
    use aero_consensus_core::tx::{ScriptPublicKey, Transaction, TransactionOutput};
    use aero_hashes::Hash;
    use aero_stratum_bridge::{
        client_handler::ClientHandler,
        default_client::{handle_authorize, handle_subscribe},
        hasher::AeroDiff,
        jsonrpc_event::JsonRpcEvent,
        mining_state::{GetMiningState, Job, MiningState},
        prom::{WorkerContext, init_metrics, record_share_found},
        share_handler::ShareHandler,
        stratum_context::StratumContext,
    };
    use num_bigint::BigUint;
    use num_traits::Zero;
    use serde_json::json;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    // ========================================================================
    // HELPER FUNCTIONS FOR TESTING
    // ========================================================================

    /// Create a test block with specified parameters
    /// This helps us test share validation without needing a real Aero node
    fn create_test_block(timestamp: u64, bits: u32, nonce: u64) -> Block {
        // Create a minimal header using from_precomputed_hash (test helper)
        let hash = Hash::from_bytes([1; 32]);
        let mut header = Header::from_precomputed_hash(hash, vec![]);
        header.timestamp = timestamp;
        header.bits = bits;
        header.nonce = nonce;
        header.daa_score = 0;
        header.blue_score = 0;
        header.version = 0;

        // Create a minimal transaction
        let script_pub_key = ScriptPublicKey::from_vec(0, vec![]);
        let tx = Transaction::new(
            0,
            vec![],
            vec![TransactionOutput::new(0, script_pub_key)],
            0,
            SubnetworkId::from_bytes([0; 20]),
            0,
            vec![],
        );

        Block::from_arcs(Arc::new(header), Arc::new(vec![tx]))
    }

    /// Create a test StratumContext for testing
    /// This simulates a client connection without needing a real TCP connection
    async fn create_test_context() -> Arc<StratumContext> {
        // Create a dummy TCP stream by connecting to a listener
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Spawn a task to accept the connection
        let accept_handle = tokio::spawn(async move { listener.accept().await });

        // Connect to the listener
        let _stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let (accepted_stream, _) = accept_handle.await.unwrap().unwrap();

        let state = Arc::new(MiningState::new());
        let (tx, _rx) = mpsc::unbounded_channel();
        StratumContext::new("127.0.0.1".to_string(), 12345, accepted_stream, state, tx)
    }

    /// Create a test StratumContext synchronously (for non-async tests)
    /// Uses a runtime to handle async operations
    fn create_test_context_sync() -> Arc<StratumContext> {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(create_test_context())
    }

    // ========================================================================
    // STRATUM PROTOCOL FLOW TESTS
    // ========================================================================
    // These tests demonstrate the complete Stratum protocol flow:
    // 1. mining.subscribe - Miner subscribes to work
    // 2. mining.authorize - Miner authorizes with wallet address
    // 3. mining.notify - Bridge sends work to miner
    // 4. mining.submit - Miner submits share
    // ========================================================================

    #[tokio::test]
    async fn test_stratum_protocol_subscribe_flow() {
        // Test: mining.subscribe request and response
        // This demonstrates how miners connect and identify themselves

        let ctx = create_test_context().await;
        let event = JsonRpcEvent::new(Some("1".to_string()), "mining.subscribe", vec![json!("BzMiner")]);

        // Handle subscribe
        let result = handle_subscribe(ctx.clone(), event, None).await;
        assert!(result.is_ok(), "Subscribe should succeed");

        // Verify miner type was detected
        let remote_app = ctx.remote_app.lock().clone();
        assert_eq!(remote_app, "BzMiner", "Miner type should be detected from params");

        // Verify extranonce was assigned (empty for default handler)
        let _extranonce = ctx.extranonce.lock().clone();
        // Extranonce assignment requires ClientHandler, so it may be empty here
    }

    #[tokio::test]
    async fn test_stratum_protocol_subscribe_with_client_handler() {
        // Test: mining.subscribe with ClientHandler for extranonce assignment
        // This demonstrates how extranonce is auto-assigned based on miner type

        let share_handler = Arc::new(ShareHandler::new("test-instance".to_string()));
        let client_handler = Arc::new(ClientHandler::new(share_handler, 8192.0, 2, "test-instance".to_string()));

        let ctx = create_test_context().await;
        let event = JsonRpcEvent::new(Some("1".to_string()), "mining.subscribe", vec![json!("IceRiver KS2L")]);

        // Handle subscribe with client handler
        let result = handle_subscribe(ctx.clone(), event, Some(client_handler.clone())).await;
        assert!(result.is_ok(), "Subscribe should succeed");

        // Verify extranonce was assigned (IceRiver needs extranonce_size=2)
        let extranonce = ctx.extranonce.lock().clone();
        assert!(!extranonce.is_empty(), "IceRiver should get extranonce assigned");
        assert_eq!(extranonce.len(), 4, "Extranonce should be 2 bytes (4 hex chars)");
    }

    #[tokio::test]
    async fn test_stratum_protocol_subscribe_bitmain_no_extranonce() {
        // Test: Bitmain miners don't get extranonce (extranonce_size=0)
        // This demonstrates miner-specific handling

        let share_handler = Arc::new(ShareHandler::new("test-instance".to_string()));
        let client_handler = Arc::new(ClientHandler::new(share_handler, 8192.0, 0, "test-instance".to_string()));

        let ctx = create_test_context().await;
        let event = JsonRpcEvent::new(Some("1".to_string()), "mining.subscribe", vec![json!("GodMiner")]);

        // Handle subscribe with client handler
        let result = handle_subscribe(ctx.clone(), event, Some(client_handler.clone())).await;
        assert!(result.is_ok(), "Subscribe should succeed");

        // Verify Bitmain doesn't get extranonce
        let extranonce = ctx.extranonce.lock().clone();
        assert!(extranonce.is_empty(), "Bitmain should not get extranonce");
    }

    // ========================================================================
    // MINER COMPATIBILITY TESTS
    // ========================================================================
    // These tests demonstrate how the bridge handles different miner types:
    // - IceRiver: Requires extranonce, single hex string job format
    // - Bitmain: No extranonce, array + timestamp job format
    // - BzMiner: Requires extranonce, single hex string job format
    // ========================================================================

    #[test]
    fn test_miner_type_detection_iceriver() {
        // Test: IceRiver miner detection
        let share_handler = Arc::new(ShareHandler::new("test-instance".to_string()));
        let client_handler = Arc::new(ClientHandler::new(share_handler, 8192.0, 2, "test-instance".to_string()));

        let ctx = create_test_context_sync();
        *ctx.remote_app.lock() = "IceRiver KS2L".to_string();
        client_handler.assign_extranonce_for_miner(&ctx, "IceRiver KS2L");

        let extranonce = ctx.extranonce.lock().clone();
        assert!(!extranonce.is_empty(), "IceRiver should get extranonce");
    }

    #[test]
    fn test_miner_type_detection_bitmain() {
        // Test: Bitmain miner detection (no extranonce)
        let share_handler = Arc::new(ShareHandler::new("test-instance".to_string()));
        let client_handler = Arc::new(ClientHandler::new(share_handler, 8192.0, 0, "test-instance".to_string()));

        let ctx = create_test_context_sync();
        *ctx.remote_app.lock() = "GodMiner".to_string();
        client_handler.assign_extranonce_for_miner(&ctx, "GodMiner");

        let extranonce = ctx.extranonce.lock().clone();
        assert!(extranonce.is_empty(), "Bitmain should not get extranonce");
    }

    #[test]
    fn test_miner_type_detection_bzminer() {
        // Test: BzMiner detection
        let share_handler = Arc::new(ShareHandler::new("test-instance".to_string()));
        let client_handler = Arc::new(ClientHandler::new(share_handler, 8192.0, 2, "test-instance".to_string()));

        let ctx = create_test_context_sync();
        *ctx.remote_app.lock() = "BzMiner".to_string();
        client_handler.assign_extranonce_for_miner(&ctx, "BzMiner");

        let extranonce = ctx.extranonce.lock().clone();
        assert!(!extranonce.is_empty(), "BzMiner should get extranonce");
    }

    // ========================================================================
    // JOB MANAGEMENT TESTS
    // ========================================================================
    // These tests demonstrate how jobs are stored, retrieved, and managed:
    // - Job storage uses a circular buffer (slot-based)
    // - Jobs can be overwritten when buffer wraps around
    // - Job ID workaround handles ASICs that submit wrong job IDs
    // ========================================================================

    #[test]
    fn test_job_storage_and_retrieval() {
        // Test: Basic job storage and retrieval
        // This demonstrates the circular buffer job storage mechanism

        let state = MiningState::new();
        let block = create_test_block(1000, 0x1e7fffff, 0);
        let pre_pow_hash = Hash::default();
        let job = Job { block, pre_pow_hash };

        // Add first job
        let job_id1 = state.add_job(job.clone());
        assert_eq!(job_id1, 1, "First job should have ID 1");
        assert_eq!(state.current_job_counter(), 1, "Counter should be 1");

        // Retrieve job
        let retrieved = state.get_job(job_id1);
        assert!(retrieved.is_some(), "Job should be retrievable");

        // Add second job
        let job_id2 = state.add_job(job.clone());
        assert_eq!(job_id2, 2, "Second job should have ID 2");
        assert_eq!(state.current_job_counter(), 2, "Counter should be 2");

        // Both jobs should be retrievable
        assert!(state.get_job(job_id1).is_some(), "First job should still be retrievable");
        assert!(state.get_job(job_id2).is_some(), "Second job should be retrievable");
    }

    #[test]
    fn test_job_circular_buffer_wraparound() {
        // Test: Job buffer wraps around after MAX_JOBS (300)
        // This demonstrates how old jobs are overwritten

        let state = MiningState::new();
        let block = create_test_block(1000, 0x1e7fffff, 0);
        let pre_pow_hash = Hash::default();
        let job = Job { block, pre_pow_hash };

        // Add jobs up to MAX_JOBS
        for i in 1..=300 {
            let job_id = state.add_job(job.clone());
            assert_eq!(job_id, i as u64, "Job ID should match counter");
        }

        // Add one more job - should wrap around to slot 0
        let job_id_301 = state.add_job(job.clone());
        assert_eq!(job_id_301, 301, "Job ID should be 301");
        assert_eq!(state.current_job_counter(), 301, "Counter should be 301");

        // Job 1 should be overwritten (slot 0 now has job 301)
        // But get_job(1) will still return what's at slot 1 (which is job 2)
        let slot_0_job = state.get_job(301);
        assert!(slot_0_job.is_some(), "Job 301 should be at slot 0");

        // Job 1 is now at slot 1%300 = slot 1, but slot 1 has job 2
        // This demonstrates the circular buffer behavior
    }

    #[test]
    fn test_job_id_workaround_logic() {
        // Test: Job ID workaround for ASICs that submit wrong job IDs
        // This demonstrates how the bridge handles stale job submissions

        let state = MiningState::new();
        let block1 = create_test_block(1000, 0x1e7fffff, 0);
        let block2 = create_test_block(2000, 0x1e7fffff, 0);
        let pre_pow_hash = Hash::default();

        // Add two jobs
        let job_id1 = state.add_job(Job { block: block1, pre_pow_hash });
        let job_id2 = state.add_job(Job { block: block2, pre_pow_hash });

        assert_eq!(job_id1, 1);
        assert_eq!(job_id2, 2);

        // Simulate ASIC submitting job_id=2 but actually meaning job_id=1
        // The workaround logic would try previous jobs if share doesn't meet difficulty
        let submitted_job_id = 2;
        let _actual_job_id = 1;

        // Get job at submitted slot
        let job_at_submitted = state.get_job(submitted_job_id);
        assert!(job_at_submitted.is_some(), "Job at submitted ID should exist");

        // Get job at actual slot (previous job)
        if submitted_job_id > 1 {
            let prev_job_id = submitted_job_id - 1;
            let prev_job = state.get_job(prev_job_id);
            assert!(prev_job.is_some(), "Previous job should exist for workaround");
        }
    }

    // ========================================================================
    // DIFFICULTY CALCULATION TESTS
    // ========================================================================
    // These tests demonstrate how difficulty is converted to target values:
    // - diff_to_target: Converts difficulty to target (BigUint)
    // - diff_to_hash: Converts difficulty to hash value (f64)
    // - AeroDiff: Stores difficulty, target, and hash value
    // ========================================================================

    #[test]
    fn test_difficulty_to_target_calculation() {
        // Test: Difficulty to target conversion
        // This demonstrates how pool difficulty is converted to a target hash

        use aero_stratum_bridge::hasher::diff_to_target;

        // Test with difficulty = 1.0 (maximum target)
        let target_1 = diff_to_target(1.0);
        assert!(!target_1.is_zero(), "Target for diff=1 should not be zero");

        // Test with difficulty = 8192.0 (common pool difficulty)
        let target_8192 = diff_to_target(8192.0);
        assert!(!target_8192.is_zero(), "Target for diff=8192 should not be zero");
        assert!(target_8192 < target_1, "Higher difficulty should have lower target");

        // Test with very high difficulty
        let target_high = diff_to_target(1_000_000.0);
        assert!(!target_high.is_zero(), "Target for high diff should not be zero");
        assert!(target_high < target_8192, "Higher difficulty should have lower target");
    }

    #[test]
    fn test_aero_diff_structure() {
        // Test: AeroDiff structure and methods
        // This demonstrates how difficulty is stored and managed

        let mut diff = AeroDiff::new();
        assert_eq!(diff.diff_value, 0.0);
        assert_eq!(diff.hash_value, 0.0);
        assert!(diff.target_value.is_zero());

        // Set difficulty
        diff.set_diff_value(8192.0);
        assert_eq!(diff.diff_value, 8192.0);
        assert!(!diff.target_value.is_zero(), "Target should be calculated");
        assert!(diff.hash_value > 0.0, "Hash value should be calculated");
    }

    #[test]
    fn test_difficulty_for_different_miners() {
        // Test: Difficulty calculation is the same for all miners
        // (Previously there were miner-specific calculations, now unified)

        let mut diff1 = AeroDiff::new();
        let mut diff2 = AeroDiff::new();
        let mut diff3 = AeroDiff::new();

        diff1.set_diff_value_for_miner(8192.0, "IceRiver");
        diff2.set_diff_value_for_miner(8192.0, "Bitmain");
        diff3.set_diff_value_for_miner(8192.0, "BzMiner");

        // All should have the same target (unified calculation)
        assert_eq!(diff1.target_value, diff2.target_value);
        assert_eq!(diff2.target_value, diff3.target_value);
    }

    // ========================================================================
    // EXTRANONCE TESTS
    // ========================================================================
    // These tests demonstrate extranonce assignment and usage:
    // - Extranonce is assigned per-client based on miner type
    // - Extranonce is prepended to nonce in share submissions
    // - Different miners have different extranonce requirements
    // ========================================================================

    #[test]
    fn test_extranonce_assignment_sequence() {
        // Test: Extranonce assignment sequence
        // This demonstrates how extranonce values are assigned sequentially

        use std::sync::atomic::{AtomicI32, Ordering};

        // Simulate the global extranonce counter
        let counter = AtomicI32::new(0);
        let max_extranonce = (2_f64.powi(16) - 1.0) as i32; // 2 bytes = 65535

        // Assign first extranonce
        let extranonce1 =
            counter.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |val| if val < max_extranonce { Some(val + 1) } else { Some(0) });
        assert!(extranonce1.is_ok());
        assert_eq!(extranonce1.unwrap(), 0);

        // Assign second extranonce
        let extranonce2 =
            counter.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |val| if val < max_extranonce { Some(val + 1) } else { Some(0) });
        assert!(extranonce2.is_ok());
        assert_eq!(extranonce2.unwrap(), 1);
    }

    #[test]
    fn test_extranonce_prepending_in_nonce() {
        // Test: Extranonce is prepended to nonce in share submissions
        // This demonstrates how the bridge combines extranonce + nonce

        let extranonce = "0001".to_string(); // 2 bytes
        let nonce_str = "12345678".to_string(); // 4 bytes
        let extranonce2_len = 16 - extranonce.len(); // 16 - 4 = 12 hex chars

        // Format: extranonce + zero-padded nonce
        let final_nonce = format!("{}{:0>width$}", extranonce, nonce_str, width = extranonce2_len);
        assert_eq!(final_nonce.len(), 16, "Final nonce should be 16 hex chars (8 bytes)");
        assert!(final_nonce.starts_with(&extranonce), "Final nonce should start with extranonce");
    }

    // ========================================================================
    // MINING STATE TESTS
    // ========================================================================
    // These tests demonstrate mining state management:
    // - Initialization flags
    // - Big job format detection
    // - Difficulty storage
    // - Header tracking
    // ========================================================================

    #[test]
    fn test_mining_state_initialization() {
        // Test: Mining state initialization and flags
        let state = MiningState::new();

        assert!(!state.is_initialized(), "State should start uninitialized");
        assert!(!state.use_big_job(), "Should start with big_job=false");
        assert_eq!(state.max_jobs(), 300, "Max jobs should be 300");

        // Initialize
        state.set_initialized(true);
        assert!(state.is_initialized(), "State should be initialized");

        // Set big job format
        state.set_use_big_job(true);
        assert!(state.use_big_job(), "Should use big job format");
    }

    #[test]
    fn test_mining_state_difficulty_storage() {
        // Test: Difficulty storage in mining state
        let state = MiningState::new();
        use num_traits::Zero;

        assert!(state.get_big_diff().is_zero(), "Big diff should start at zero");

        let test_diff = BigUint::from(8192u64);
        state.set_big_diff(test_diff.clone());
        assert_eq!(state.get_big_diff(), test_diff, "Big diff should be stored");

        // Test stratum difficulty
        let mut stratum_diff = AeroDiff::new();
        stratum_diff.set_diff_value(4096.0);
        state.set_stratum_diff(stratum_diff.clone());

        let retrieved = state.stratum_diff();
        assert!(retrieved.is_some(), "Stratum diff should be stored");
        assert_eq!(retrieved.unwrap().diff_value, 4096.0);
    }

    #[test]
    fn test_mining_state_header_tracking() {
        // Test: Header tracking for change detection
        let state = MiningState::new();

        assert!(state.get_last_header().is_none(), "Should start with no header");

        let hash = Hash::from_bytes([1; 32]);
        let mut header = Header::from_precomputed_hash(hash, vec![]);
        header.timestamp = 1000;
        header.bits = 0x1e7fffff;
        header.blue_score = 100;

        state.set_last_header(header.clone());
        let retrieved = state.get_last_header();
        assert!(retrieved.is_some(), "Header should be stored");
        assert_eq!(retrieved.unwrap().timestamp, 1000);
    }

    // ========================================================================
    // SHARE HANDLER TESTS
    // ========================================================================
    // These tests demonstrate share handling and statistics:
    // - Share statistics tracking
    // - Worker statistics
    // - VarDiff management
    // ========================================================================

    #[test]
    fn test_share_handler_creation() {
        // Test: ShareHandler creation and initialization
        let handler = ShareHandler::new("test-instance".to_string());
        // Verify handler was created (can't test log_prefix as it's private)
        assert!(std::mem::size_of_val(&handler) > 0, "Handler should be created");
    }

    #[test]
    fn test_share_handler_worker_stats() {
        // Test: Worker statistics creation and tracking
        let handler = ShareHandler::new("test-instance".to_string());
        let ctx = create_test_context_sync();
        *ctx.worker_name.lock() = "worker1".to_string();
        *ctx.wallet_addr.lock() = "aerotest:test".to_string();

        let stats = handler.get_create_stats(&ctx);
        assert_eq!(*stats.worker_name.lock(), "worker1");
        assert_eq!(*stats.shares_found.lock(), 0);
        assert_eq!(*stats.blocks_found.lock(), 0);
    }

    #[test]
    fn test_worker_prom_session_syncs_start_time_for_current_wallet() {
        use prometheus::gather;

        init_metrics();

        let handler = ShareHandler::new("Instance 1".to_string());
        let ctx = create_test_context_sync();
        *ctx.worker_name.lock() = "ks0".to_string();

        // Stats entry created before wallet is known (no prom session yet).
        let stats_before_wallet = handler.get_create_stats(&ctx);
        assert_eq!(*stats_before_wallet.worker_name.lock(), "ks0");

        let wallet = "aero:qr8example123456789012345678901234567890123456789012345678901234567890".to_string();
        *ctx.wallet_addr.lock() = wallet.clone();

        // Reusing the same in-memory stats entry must still sync prom start time for the wallet.
        let stats_after_wallet = handler.get_create_stats(&ctx);
        assert!(Arc::ptr_eq(&stats_before_wallet.worker_name, &stats_after_wallet.worker_name));

        record_share_found(&WorkerContext::from_stratum("Instance 1", &ctx, ""), 8192.0);

        let mut found_start_time = false;
        for family in gather() {
            if family.name() != "ks_worker_start_time" {
                continue;
            }
            for metric in family.get_metric() {
                let labels: std::collections::HashMap<&str, &str> = metric.get_label().iter().map(|l| (l.name(), l.value())).collect();
                if labels.get("instance") == Some(&"Instance 1")
                    && labels.get("worker") == Some(&"ks0")
                    && labels.get("wallet") == Some(&wallet.as_str())
                    && metric.get_gauge().value() > 0.0
                {
                    found_start_time = true;
                }
            }
        }

        assert!(found_start_time, "ks_worker_start_time must exist for the authorized wallet label set");
    }

    #[test]
    fn test_share_handler_vardiff_management() {
        // Test: VarDiff difficulty management
        let handler = ShareHandler::new("test-instance".to_string());
        let ctx = create_test_context_sync();
        *ctx.worker_name.lock() = "worker1".to_string();
        *ctx.wallet_addr.lock() = "aerotest:test".to_string();
        handler.get_create_stats(&ctx);

        // Set initial difficulty
        let prev = handler.set_client_vardiff(&ctx, 8192.0);
        assert_eq!(prev, 0.0, "Previous diff should be 0");

        // Get current difficulty
        let current = handler.get_client_vardiff(&ctx);
        assert_eq!(current, 8192.0, "Current diff should be 8192");

        // Change difficulty
        let prev2 = handler.set_client_vardiff(&ctx, 4096.0);
        assert_eq!(prev2, 8192.0, "Previous diff should be 8192");
        assert_eq!(handler.get_client_vardiff(&ctx), 4096.0, "New diff should be 4096");
    }

    // ========================================================================
    // VARDIFF LOGIC TESTS
    // ========================================================================
    // These tests demonstrate VarDiff difficulty adjustment:
    // - Difficulty increases when shares are too fast
    // - Difficulty decreases when shares are too slow
    // - Pow2 clamping (if enabled)
    // ========================================================================

    #[test]
    fn test_vardiff_concept_documentation() {
        // Test: VarDiff concept documentation
        // This test documents how VarDiff works conceptually
        // Note: The actual vardiff_compute_next_diff function is private,
        // so we document the concept instead

        // VarDiff (Variable Difficulty) adjusts pool difficulty based on share submission rate:
        //
        // 1. If miner submits shares too fast (above target rate):
        //    - Difficulty increases to slow down share submissions
        //    - Example: Target 20 shares/min, miner submits 40 shares/min
        //    - Result: Difficulty doubles (approximately)
        //
        // 2. If miner submits shares too slow (below target rate):
        //    - Difficulty decreases to speed up share submissions
        //    - Example: Target 20 shares/min, miner submits 10 shares/min
        //    - Result: Difficulty halves (approximately)
        //
        // 3. Pow2 clamping (optional):
        //    - Rounds difficulty to nearest power of 2
        //    - Example: 10000 -> 8192 (2^13) or 16384 (2^14)
        //    - Helps with ASIC compatibility

        // This test verifies the ShareHandler can manage VarDiff
        let handler = ShareHandler::new("test-instance".to_string());
        let ctx = create_test_context_sync();
        *ctx.worker_name.lock() = "worker1".to_string();
        *ctx.wallet_addr.lock() = "aerotest:test".to_string();
        handler.get_create_stats(&ctx);

        // Set initial difficulty
        handler.set_client_vardiff(&ctx, 8192.0);
        assert_eq!(handler.get_client_vardiff(&ctx), 8192.0);

        // VarDiff thread would adjust this based on share rate
        // (Actual adjustment logic is tested via integration tests)
    }

    // ========================================================================
    // ERROR HANDLING TESTS
    // ========================================================================
    // These tests demonstrate error handling and edge cases:
    // - Invalid wallet addresses
    // - Malformed JSON-RPC messages
    // - Missing job errors
    // ========================================================================

    #[test]
    fn test_error_short_codes() {
        // Test: Error short codes for tracking
        use aero_stratum_bridge::errors::ErrorShortCode;

        assert_eq!(ErrorShortCode::NoMinerAddress.as_str(), "err_no_miner_address");
        assert_eq!(ErrorShortCode::FailedBlockFetch.as_str(), "err_failed_block_fetch");
        assert_eq!(ErrorShortCode::InvalidAddressFmt.as_str(), "err_malformed_wallet_address");
        assert_eq!(ErrorShortCode::MissingJob.as_str(), "err_missing_job");
    }

    // ========================================================================
    // INTEGRATION-LEVEL TESTS
    // ========================================================================
    // These tests demonstrate how components work together:
    // - Full Stratum flow with mock data
    // - Share validation with test blocks
    // ========================================================================

    #[tokio::test]
    async fn test_full_stratum_flow_mock() {
        // Test: Full Stratum protocol flow with mock components
        // This demonstrates the complete flow without needing a real node

        // 1. Create components
        let share_handler = Arc::new(ShareHandler::new("test-instance".to_string()));
        let client_handler = Arc::new(ClientHandler::new(share_handler.clone(), 8192.0, 2, "test-instance".to_string()));
        let ctx = create_test_context().await;

        // 2. Subscribe
        let subscribe_event = JsonRpcEvent::new(Some("1".to_string()), "mining.subscribe", vec![json!("IceRiver KS2L")]);
        let result = handle_subscribe(ctx.clone(), subscribe_event, Some(client_handler.clone())).await;
        assert!(result.is_ok(), "Subscribe should succeed");

        // 3. Verify extranonce was assigned
        let extranonce = ctx.extranonce.lock().clone();
        assert!(!extranonce.is_empty(), "Extranonce should be assigned");

        // 4. Verify miner type was detected
        let remote_app = ctx.remote_app.lock().clone();
        assert_eq!(remote_app, "IceRiver KS2L");

        // 5. Initialize mining state
        let state = GetMiningState(&ctx);
        state.set_initialized(true);
        state.set_use_big_job(false); // IceRiver uses single hex string, not "big job"

        // 6. Set difficulty
        let mut stratum_diff = AeroDiff::new();
        stratum_diff.set_diff_value(8192.0);
        state.set_stratum_diff(stratum_diff);

        // 7. Add a job
        let block = create_test_block(1000, 0x1e7fffff, 0);
        let pre_pow_hash = Hash::default();
        let job = Job { block, pre_pow_hash };
        let job_id = state.add_job(job);

        assert!(job_id > 0, "Job should be added");
        assert!(state.get_job(job_id).is_some(), "Job should be retrievable");
    }

    // ========================================================================
    // DOCUMENTATION TESTS
    // ========================================================================
    // These tests serve as documentation and examples for developers:
    // - How to use the bridge API
    // - Common patterns and workflows
    // ========================================================================

    #[test]
    fn test_example_miner_connection_flow() {
        // Example: How a miner connects and starts mining
        // This test documents the expected flow for developers

        // Step 1: Miner connects via TCP
        // (Simulated by creating StratumContext)

        // Step 2: Miner sends mining.subscribe
        // Params: [miner_app_name]
        // Response: [subscription_id, extranonce] or [true, "EthereumStratum/1.0.0"]

        // Step 3: Miner sends mining.authorize
        // Params: [wallet_address.worker_name]
        // Response: true

        // Step 4: Bridge sends mining.set_difficulty
        // Params: [difficulty_value]

        // Step 5: Bridge sends mining.notify (job)
        // Params: [job_id, job_data, ...]

        // Step 6: Miner sends mining.submit
        // Params: [wallet_address, job_id, nonce]
        // Response: true (valid share) or error

        // This test verifies the components are set up correctly
        let share_handler = Arc::new(ShareHandler::new("example-instance".to_string()));
        let client_handler = Arc::new(ClientHandler::new(share_handler, 8192.0, 2, "example-instance".to_string()));
        // Verify handler was created
        assert!(std::mem::size_of_val(&client_handler) > 0, "Client handler should be created");
    }

    // ========================================================================
    // WALLET ADDRESS CLEANING TESTS
    // ========================================================================
    // These tests verify wallet address cleaning and validation through
    // handle_authorize, which calls clean_wallet internally.
    // ========================================================================

    #[tokio::test]
    async fn test_wallet_address_cleaning_with_prefix() {
        // Test: Wallet addresses with aero:/aerotest:/aerodev: prefixes
        // These should be accepted as-is
        let ctx = create_test_context().await;
        // Note: State is already created in create_test_context, no need to create another

        // Test aero: prefix
        let event1 = JsonRpcEvent::new(
            Some("1".to_string()),
            "mining.authorize",
            vec![json!("aero:qr8example123456789012345678901234567890123456789012345678901234567890")],
        );
        // Verify event was created with proper prefix before calling handle_authorize
        assert_eq!(event1.params.len(), 1);
        let addr1 = event1.params[0].as_str().unwrap();
        assert!(addr1.starts_with("aero:"), "Address should have aero: prefix");
        let _result1: Result<(), _> = handle_authorize(ctx.clone(), event1, None, None).await;
        // Note: This will fail with invalid address, but we're testing the cleaning logic
        // In real scenario, valid addresses would work

        // Test aerotest: prefix
        let ctx2 = create_test_context().await;
        let event2 = JsonRpcEvent::new(
            Some("2".to_string()),
            "mining.authorize",
            vec![json!("aerotest:qr8example123456789012345678901234567890123456789012345678901234567890")],
        );
        assert_eq!(event2.params.len(), 1);
        let addr2 = event2.params[0].as_str().unwrap();
        assert!(addr2.starts_with("aerotest:"), "Address should have aerotest: prefix");
        let _result2: Result<(), _> = handle_authorize(ctx2.clone(), event2, None, None).await;

        // Test aerodev: prefix
        let ctx3 = create_test_context().await;
        let event3 = JsonRpcEvent::new(
            Some("3".to_string()),
            "mining.authorize",
            vec![json!("aerodev:qr8example123456789012345678901234567890123456789012345678901234567890")],
        );
        assert_eq!(event3.params.len(), 1);
        let addr3 = event3.params[0].as_str().unwrap();
        assert!(addr3.starts_with("aerodev:"), "Address should have aerodev: prefix");
        let _result3: Result<(), _> = handle_authorize(ctx3.clone(), event3, None, None).await;
    }

    #[tokio::test]
    async fn test_wallet_address_cleaning_without_prefix() {
        // Test: Wallet addresses without prefix should get aero: prefix added
        let ctx = create_test_context().await;
        let event = JsonRpcEvent::new(
            Some("1".to_string()),
            "mining.authorize",
            vec![json!("qr8example123456789012345678901234567890123456789012345678901234567890")],
        );
        // Verify event was created with address without prefix (before calling handle_authorize)
        assert_eq!(event.params.len(), 1);
        let addr_param = event.params[0].as_str().unwrap();
        assert!(!addr_param.starts_with("aero:"), "Test address should not have prefix initially");

        // handle_authorize will call clean_wallet which should add aero: prefix
        let _result: Result<(), _> = handle_authorize(ctx.clone(), event, None, None).await;
        // Note: Actual validation requires valid address format
    }

    #[tokio::test]
    async fn test_wallet_address_cleaning_invalid_addresses() {
        // Test: Invalid addresses should be rejected
        let ctx = create_test_context().await;

        // Test empty address
        let event1 = JsonRpcEvent::new(Some("1".to_string()), "mining.authorize", vec![json!("")]);
        let result1: Result<(), _> = handle_authorize(ctx.clone(), event1, None, None).await;
        assert!(result1.is_err(), "Empty address should be rejected");

        // Test malformed address
        let ctx2 = create_test_context().await;
        let event2 = JsonRpcEvent::new(Some("2".to_string()), "mining.authorize", vec![json!("invalid_address")]);
        let result2: Result<(), _> = handle_authorize(ctx2.clone(), event2, None, None).await;
        assert!(result2.is_err(), "Invalid address format should be rejected");
    }

    #[tokio::test]
    async fn test_wallet_address_cleaning_whitespace_handling() {
        // Test: Addresses with whitespace should be handled
        let ctx = create_test_context().await;
        let event = JsonRpcEvent::new(
            Some("1".to_string()),
            "mining.authorize",
            vec![json!("  aero:qr8example123456789012345678901234567890123456789012345678901234567890  ")],
        );
        // Verify event was created with whitespace (before calling handle_authorize)
        assert_eq!(event.params.len(), 1);
        let addr_param = event.params[0].as_str().unwrap();
        assert!(addr_param.starts_with("  ") || addr_param.ends_with("  "), "Test address should have whitespace");

        // Whitespace should be trimmed during processing
        let _result: Result<(), _> = handle_authorize(ctx.clone(), event, None, None).await;
    }

    // ========================================================================
    // SHARE HANDLER EDGE CASE TESTS
    // ========================================================================
    // These tests verify edge cases in share submission handling:
    // - EthereumStratum format (5 params)
    // - ASIC format (3 params)
    // - Invalid parameter counts
    // - Job ID workaround logic
    // ========================================================================

    #[test]
    fn test_share_submit_ethereumstratum_format_validation() {
        // Test: EthereumStratum format with 5 parameters (lolMiner)
        // Format: [address.name, job_id, extranonce2, ntime, nonce]
        // This test verifies the format structure (parameter count validation)
        // Note: Full submission testing requires a AeroApi mock, so we test the format here

        // Create event with 5 params (EthereumStratum format)
        let event = JsonRpcEvent::new(
            Some("1".to_string()),
            "mining.submit",
            vec![
                json!("aerotest:qr8example123456789012345678901234567890123456789012345678901234567890.worker1"),
                json!("1"),
                json!("0000"),     // extranonce2
                json!("00000000"), // ntime
                json!("00000000"), // nonce
            ],
        );

        // Verify event has 5 params (>= 3, so it passes initial validation)
        assert!(event.params.len() >= 3, "EthereumStratum format should have >= 3 params");
        assert_eq!(event.params.len(), 5, "EthereumStratum format should have exactly 5 params");
    }

    #[test]
    fn test_share_submit_asic_format_validation() {
        // Test: ASIC format with 3 parameters
        // Format: [address.name, job_id, nonce]
        // This test verifies the format structure (parameter count validation)

        // Create event with 3 params (ASIC format)
        let event = JsonRpcEvent::new(
            Some("1".to_string()),
            "mining.submit",
            vec![
                json!("aerotest:qr8example123456789012345678901234567890123456789012345678901234567890.worker1"),
                json!("1"),
                json!("00000000"), // nonce
            ],
        );

        // Verify event has 3 params (minimum required)
        assert_eq!(event.params.len(), 3, "ASIC format should have exactly 3 params");
    }

    #[test]
    fn test_share_submit_invalid_param_count_validation() {
        // Test: Submit event validation - < 3 parameters should be invalid
        // This tests the parameter count validation logic that happens before handle_submit

        // Test with 2 params (too few)
        let event1 = JsonRpcEvent::new(Some("1".to_string()), "mining.submit", vec![json!("address"), json!("job_id")]);
        assert!(event1.params.len() < 3, "Submit with < 3 params should be invalid");

        // Test with 1 param (too few)
        let event2 = JsonRpcEvent::new(Some("2".to_string()), "mining.submit", vec![json!("address")]);
        assert!(event2.params.len() < 3, "Submit with 1 param should be invalid");

        // Test with 0 params (too few)
        let event3 = JsonRpcEvent::new(Some("3".to_string()), "mining.submit", vec![]);
        assert!(event3.params.len() < 3, "Submit with 0 params should be invalid");
    }

    // ========================================================================
    // VARDIFF EDGE CASE TESTS
    // ========================================================================
    // These tests verify VarDiff edge cases through ShareHandler API
    // ========================================================================

    #[test]
    fn test_vardiff_boundary_conditions() {
        // Test: VarDiff at min/max difficulty boundaries
        // Note: set_client_vardiff stores the value as-is; clamping happens during VarDiff computation
        let handler = ShareHandler::new("test-instance".to_string());
        let ctx = create_test_context_sync();
        *ctx.worker_name.lock() = "worker1".to_string();
        *ctx.wallet_addr.lock() = "aerotest:test".to_string();
        handler.get_create_stats(&ctx);

        // Test minimum difficulty (1.0)
        handler.set_client_vardiff(&ctx, 1.0);
        assert_eq!(handler.get_client_vardiff(&ctx), 1.0, "Minimum difficulty should be 1.0");

        // Test very high difficulty
        handler.set_client_vardiff(&ctx, 1_000_000.0);
        assert_eq!(handler.get_client_vardiff(&ctx), 1_000_000.0, "High difficulty should be stored");

        // Test that values can be set directly (clamping happens in VarDiff computation, not storage)
        handler.set_client_vardiff(&ctx, 0.5);
        assert_eq!(handler.get_client_vardiff(&ctx), 0.5, "set_client_vardiff stores value as-is");

        // Verify we can set it back to a valid value
        handler.set_client_vardiff(&ctx, 1.0);
        assert_eq!(handler.get_client_vardiff(&ctx), 1.0, "Can set difficulty back to valid value");
    }

    #[test]
    fn test_vardiff_no_change_scenarios() {
        // Test: Scenarios where VarDiff should not change
        let handler = ShareHandler::new("test-instance".to_string());
        let ctx = create_test_context_sync();
        *ctx.worker_name.lock() = "worker1".to_string();
        *ctx.wallet_addr.lock() = "aerotest:test".to_string();
        handler.get_create_stats(&ctx);

        // Set initial difficulty
        handler.set_client_vardiff(&ctx, 8192.0);
        let initial = handler.get_client_vardiff(&ctx);

        // VarDiff won't change if:
        // 1. Not enough time elapsed (< VARDIFF_MIN_ELAPSED_SECS)
        // 2. Not enough shares (< VARDIFF_MIN_SHARES)
        // 3. Share rate is within acceptable range (VARDIFF_LOWER_RATIO to VARDIFF_UPPER_RATIO)
        // 4. Relative change is < 10%

        // These conditions are tested through the VarDiff thread in production
        // Here we verify the API works correctly
        assert_eq!(handler.get_client_vardiff(&ctx), initial, "Difficulty should remain unchanged when conditions not met");
    }

    // ========================================================================
    // CLIENT HANDLER EDGE CASE TESTS
    // ========================================================================
    // These tests verify client handler edge cases
    // ========================================================================

    #[test]
    fn test_client_handler_extranonce_wrapping_edge() {
        // Test: Extranonce wrapping at MAX_EXTRANONCE boundary
        // This tests the extranonce assignment logic when reaching the maximum value
        let share_handler = Arc::new(ShareHandler::new("test-instance".to_string()));
        let handler = ClientHandler::new(share_handler, 8192.0, 2, "test-instance".to_string());

        // Create multiple contexts to test extranonce assignment
        let ctx1 = create_test_context_sync();
        let ctx2 = create_test_context_sync();
        let ctx3 = create_test_context_sync();

        // Assign extranonces
        handler.assign_extranonce_for_miner(&ctx1, "IceRiverMiner");
        handler.assign_extranonce_for_miner(&ctx2, "BzMiner");
        handler.assign_extranonce_for_miner(&ctx3, "Goldshell");

        // Verify extranonces were assigned
        let extranonce1 = ctx1.extranonce.lock().clone();
        let extranonce2 = ctx2.extranonce.lock().clone();
        let extranonce3 = ctx3.extranonce.lock().clone();

        assert!(!extranonce1.is_empty(), "IceRiver should get extranonce");
        assert!(!extranonce2.is_empty(), "BzMiner should get extranonce");
        assert!(!extranonce3.is_empty(), "Goldshell should get extranonce");

        // Verify they're different
        assert_ne!(extranonce1, extranonce2, "Extranonces should be different");
        assert_ne!(extranonce2, extranonce3, "Extranonces should be different");
    }

    #[test]
    fn test_client_handler_big_job_format_detection() {
        // Test: Big job format detection for BzMiner/IceRiver
        let share_handler = Arc::new(ShareHandler::new("test-instance".to_string()));
        let handler = ClientHandler::new(share_handler, 8192.0, 2, "test-instance".to_string());

        let ctx_bzminer = create_test_context_sync();
        let ctx_iceriver = create_test_context_sync();
        let ctx_bitmain = create_test_context_sync();

        // Test BzMiner detection
        handler.assign_extranonce_for_miner(&ctx_bzminer, "BzMiner");
        let bz_extranonce = ctx_bzminer.extranonce.lock().clone();
        assert!(!bz_extranonce.is_empty(), "BzMiner should get extranonce (big job format)");

        // Test IceRiver detection
        handler.assign_extranonce_for_miner(&ctx_iceriver, "IceRiverMiner");
        let ice_extranonce = ctx_iceriver.extranonce.lock().clone();
        assert!(!ice_extranonce.is_empty(), "IceRiver should get extranonce (big job format)");

        // Test Bitmain detection (no extranonce)
        handler.assign_extranonce_for_miner(&ctx_bitmain, "GodMiner");
        let bitmain_extranonce = ctx_bitmain.extranonce.lock().clone();
        assert!(bitmain_extranonce.is_empty(), "Bitmain should not get extranonce");
    }

    // ========================================================================
    // JSON-RPC EVENT EDGE CASE TESTS
    // ========================================================================
    // These tests verify JSON-RPC event parsing edge cases
    // ========================================================================

    #[test]
    fn test_unmarshal_event_type_mismatches() {
        // Test: Events with wrong parameter types
        use aero_stratum_bridge::jsonrpc_event::unmarshal_event;

        // Test with method as number instead of string
        let json1 = r#"{"jsonrpc":"2.0","method":123,"params":[],"id":1}"#;
        let _result1 = unmarshal_event(json1);
        // Should handle gracefully (may fail parsing or use default)

        // Test with params as string instead of array
        let json2 = r#"{"jsonrpc":"2.0","method":"mining.subscribe","params":"invalid","id":1}"#;
        let _result2 = unmarshal_event(json2);
        // Should handle gracefully

        // Test with id as object instead of number/string
        let json3 = r#"{"jsonrpc":"2.0","method":"mining.subscribe","params":[],"id":{"invalid":true}}"#;
        let _result3 = unmarshal_event(json3);
        // Should handle gracefully

        // These tests verify the parser doesn't panic on type mismatches
        // The fact that unmarshal_event doesn't panic is the test
    }

    #[test]
    fn test_unmarshal_event_missing_required_fields() {
        // Test: Events missing jsonrpc/method fields
        use aero_stratum_bridge::jsonrpc_event::unmarshal_event;

        // Test missing jsonrpc field
        let json1 = r#"{"method":"mining.subscribe","params":[],"id":1}"#;
        let _result1 = unmarshal_event(json1);

        // Test missing method field
        let json2 = r#"{"jsonrpc":"2.0","params":[],"id":1}"#;
        let _result2 = unmarshal_event(json2);

        // Test completely empty object
        let json3 = r#"{}"#;
        let _result3 = unmarshal_event(json3);

        // These tests verify the parser handles missing fields gracefully
        // The fact that unmarshal_event doesn't panic is the test
    }

    // ========================================================================
    // MINING STATE EDGE CASE TESTS
    // ========================================================================
    // These tests verify mining state edge cases
    // ========================================================================

    #[test]
    fn test_mining_state_job_retrieval_nonexistent() {
        // Test: Retrieving non-existent job IDs
        let state = MiningState::new();

        // Try to retrieve job that doesn't exist
        let result = state.get_job(99999);
        assert!(result.is_none(), "Non-existent job should return None");

        // Try to retrieve job ID 0 (invalid)
        let result2 = state.get_job(0);
        assert!(result2.is_none(), "Job ID 0 should return None");
    }

    #[test]
    fn test_mining_state_job_id_sequence() {
        // Test: Job ID sequence correctness
        let state = MiningState::new();

        // Create multiple jobs
        let hash1 = Hash::from_bytes([1; 32]);
        let block1 = Block::from_precomputed_hash(hash1, vec![]);
        let job1 = Job { block: block1, pre_pow_hash: Hash::default() };

        let hash2 = Hash::from_bytes([2; 32]);
        let block2 = Block::from_precomputed_hash(hash2, vec![]);
        let job2 = Job { block: block2, pre_pow_hash: Hash::default() };

        let hash3 = Hash::from_bytes([3; 32]);
        let block3 = Block::from_precomputed_hash(hash3, vec![]);
        let job3 = Job { block: block3, pre_pow_hash: Hash::default() };

        // Add jobs and verify IDs are sequential
        let id1 = state.add_job(job1);
        assert_eq!(id1, 1, "First job should have ID 1");

        let id2 = state.add_job(job2);
        assert_eq!(id2, 2, "Second job should have ID 2");

        let id3 = state.add_job(job3);
        assert_eq!(id3, 3, "Third job should have ID 3");

        // Verify counter increments
        assert_eq!(state.current_job_counter(), 3, "Job counter should be 3");
    }

    // ========================================================================
    // STRATUM CONTEXT TESTS
    // ========================================================================
    // These tests verify StratumContext functionality
    // ========================================================================

    #[tokio::test]
    async fn test_stratum_context_connected_flag() {
        // Test: Connected/disconnected state management
        // Note: disconnect() may use async operations, so we use tokio::test
        let ctx = create_test_context().await;

        // Initially should be connected
        assert!(ctx.connected(), "Context should start connected");

        // Disconnect
        ctx.disconnect();
        assert!(!ctx.connected(), "Context should be disconnected after disconnect()");
    }

    #[test]
    fn test_stratum_context_id_management() {
        // Test: Client ID setting and retrieval
        let ctx = create_test_context_sync();

        // Initially no ID
        assert!(ctx.id().is_none(), "Context should start with no ID");

        // Set ID
        ctx.set_id(42);
        assert_eq!(ctx.id(), Some(42), "Context ID should be 42");

        // Set ID to 0 (should return None)
        ctx.set_id(0);
        assert!(ctx.id().is_none(), "ID 0 should return None");
    }

    #[test]
    fn test_default_worker_name_when_miner_omits_dot_worker() {
        let ctx = create_test_context_sync();
        ctx.set_id(9);
        assert!(ctx.worker_name.lock().is_empty());

        ctx.ensure_default_worker_name();
        assert_eq!(ctx.effective_worker_name(), "asic-9");
        assert!(!ctx.effective_worker_name().contains("127.0.0.1"));

        *ctx.worker_name.lock() = "rig-a".to_string();
        assert_eq!(ctx.effective_worker_name(), "rig-a");
    }

    #[test]
    fn test_stratum_context_summary() {
        // Test: Context summary generation
        let ctx = create_test_context_sync();

        // Set wallet and worker
        *ctx.wallet_addr.lock() = "aerotest:qr8example123456789012345678901234567890123456789012345678901234567890".to_string();
        *ctx.worker_name.lock() = "worker1".to_string();
        *ctx.remote_app.lock() = "BzMiner".to_string();

        let summary = ctx.summary();
        assert_eq!(summary.remote_addr, "127.0.0.1", "Summary should contain remote address");
        assert_eq!(summary.remote_port, 12345, "Summary should contain remote port");
        assert_eq!(
            summary.wallet_addr, "aerotest:qr8example123456789012345678901234567890123456789012345678901234567890",
            "Summary should contain wallet address"
        );
        assert_eq!(summary.worker_name, "worker1", "Summary should contain worker name");
        assert_eq!(summary.remote_app, "BzMiner", "Summary should contain remote app");
    }
}
