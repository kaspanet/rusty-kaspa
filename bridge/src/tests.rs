#[cfg(test)]
use crate::cli::{parse_bool, parse_instance_spec};
#[cfg(test)]
use kaspa_stratum_bridge::BridgeConfig;

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
fn test_config_single_instance_mode() {
    let yaml = r#"
kaspad_address: "127.0.0.1:16110"
stratum_port: ":5555"
min_share_diff: 8192
print_stats: true
"#;

    let config = BridgeConfig::from_yaml(yaml);
    assert!(config.is_ok());
    let config = config.unwrap();
    assert_eq!(config.instances.len(), 1);
    assert_eq!(config.instances[0].stratum_port, ":5555");
    assert_eq!(config.instances[0].min_share_diff, 8192);
    assert_eq!(config.global.kaspad_address, "127.0.0.1:16110");
}

#[cfg(test)]
#[test]
fn test_config_single_instance_defaults_when_missing_fields() {
    let yaml = r#"
kaspad_address: "127.0.0.1:16110"
print_stats: true
"#;

    let config = BridgeConfig::from_yaml(yaml);
    assert!(config.is_ok());
    let config = config.unwrap();
    assert_eq!(config.instances.len(), 1);
    assert_eq!(config.instances[0].stratum_port, ":5555");
    assert_eq!(config.instances[0].min_share_diff, 8192);
}

#[cfg(test)]
#[test]
fn test_config_multi_instance_mode() {
    let yaml = r#"
kaspad_address: "127.0.0.1:16110"
instances:
  - stratum_port: ":5555"
    min_share_diff: 8192
  - stratum_port: ":5556"
    min_share_diff: 4096
"#;

    let config = BridgeConfig::from_yaml(yaml);
    assert!(config.is_ok());
    let config = config.unwrap();
    assert_eq!(config.instances.len(), 2);
    assert_eq!(config.instances[0].stratum_port, ":5555");
    assert_eq!(config.instances[0].min_share_diff, 8192);
    assert_eq!(config.instances[1].stratum_port, ":5556");
    assert_eq!(config.instances[1].min_share_diff, 4096);
}

#[cfg(test)]
#[test]
fn test_config_port_normalization() {
    let yaml = r#"
kaspad_address: "127.0.0.1:16110"
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
kaspad_address: "127.0.0.1:16110"
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
kaspad_address: "127.0.0.1:16110"
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
kaspad_address: "127.0.0.1:16110"
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
kaspad_address: "127.0.0.1:16110"
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
kaspad_address: "127.0.0.1:16110"
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
kaspad_address: "127.0.0.1:16110"
instances:
  - min_share_diff: 8192
"#;

    let yaml_missing_diff = r#"
kaspad_address: "127.0.0.1:16110"
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
kaspad_address: "127.0.0.1:16110"
stratum_port: ":5555"
"#;

    let yaml_single_missing_port = r#"
kaspad_address: "127.0.0.1:16110"
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
kaspad_address: "127.0.0.1:16110"
stratum_port: ":5555"
min_share_diff: 8192
web_dashboard_port: ""
"#;

    let config = BridgeConfig::from_yaml(yaml);
    assert!(config.is_ok());
    let config = config.unwrap();
    assert_eq!(config.global.web_dashboard_port, "");
}
