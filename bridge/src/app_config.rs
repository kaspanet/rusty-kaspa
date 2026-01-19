use std::collections::HashSet;
use std::time::Duration;

use yaml_rust::YamlLoader;

/// Instance-specific configuration
#[derive(Debug, Clone)]
pub(crate) struct InstanceConfig {
    pub(crate) stratum_port: String,
    pub(crate) min_share_diff: u32,
    pub(crate) prom_port: Option<String>, // Optional per-instance prom port
    pub(crate) log_to_file: Option<bool>, // Optional per-instance logging
    // Instance-specific settings that can override global defaults
    pub(crate) var_diff: Option<bool>,
    pub(crate) shares_per_min: Option<u32>,
    pub(crate) var_diff_stats: Option<bool>,
    pub(crate) pow2_clamp: Option<bool>,
}

/// Global configuration (shared across all instances)
#[derive(Debug, Clone)]
pub(crate) struct GlobalConfig {
    pub(crate) kaspad_address: String,
    pub(crate) block_wait_time: Duration,
    pub(crate) print_stats: bool,
    pub(crate) log_to_file: bool, // Default for instances that don't specify
    pub(crate) health_check_port: String,
    pub(crate) var_diff: bool,
    pub(crate) shares_per_min: u32,
    pub(crate) var_diff_stats: bool,
    pub(crate) extranonce_size: u8,
    pub(crate) pow2_clamp: bool,
    pub(crate) coinbase_tag_suffix: Option<String>,
}

/// Bridge configuration (supports both single and multi-instance modes)
#[derive(Debug)]
pub(crate) struct BridgeConfig {
    pub(crate) global: GlobalConfig,
    pub(crate) instances: Vec<InstanceConfig>,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            kaspad_address: "localhost:16110".to_string(),
            block_wait_time: Duration::from_millis(1000),
            print_stats: true,
            log_to_file: true,
            health_check_port: String::new(),
            var_diff: true,
            shares_per_min: 20,
            var_diff_stats: false,
            extranonce_size: 0,
            pow2_clamp: false,
            coinbase_tag_suffix: None,
        }
    }
}

impl Default for InstanceConfig {
    fn default() -> Self {
        Self {
            stratum_port: ":5555".to_string(),
            min_share_diff: 8192,
            prom_port: None,
            log_to_file: None,
            var_diff: None,
            shares_per_min: None,
            var_diff_stats: None,
            pow2_clamp: None,
        }
    }
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self { global: GlobalConfig::default(), instances: vec![InstanceConfig::default()] }
    }
}

impl BridgeConfig {
    pub(crate) fn from_yaml(content: &str) -> Result<Self, anyhow::Error> {
        let docs = YamlLoader::load_from_str(content)?;
        let doc = docs.first().ok_or_else(|| anyhow::anyhow!("empty YAML document"))?;

        // Parse global config
        let mut global = GlobalConfig::default();

        if let Some(addr) = doc["kaspad_address"].as_str() {
            global.kaspad_address = addr.to_string();
        }

        if let Some(stats) = doc["print_stats"].as_bool() {
            global.print_stats = stats;
        }

        if let Some(log) = doc["log_to_file"].as_bool() {
            global.log_to_file = log;
        }

        if let Some(port) = doc["health_check_port"].as_str() {
            global.health_check_port = port.to_string();
        }

        if let Some(vd) = doc["var_diff"].as_bool() {
            global.var_diff = vd;
        }

        if let Some(spm) = doc["shares_per_min"].as_i64() {
            global.shares_per_min = spm as u32;
        }

        if let Some(vds) = doc["var_diff_stats"].as_bool() {
            global.var_diff_stats = vds;
        }

        if let Some(ens) = doc["extranonce_size"].as_i64() {
            global.extranonce_size = ens as u8;
        }

        if let Some(clamp) = doc["pow2_clamp"].as_bool() {
            global.pow2_clamp = clamp;
        }

        if let Some(suffix) = doc["coinbase_tag_suffix"].as_str() {
            let suffix = suffix.trim();
            global.coinbase_tag_suffix = if suffix.is_empty() { None } else { Some(suffix.to_string()) };
        }

        // Parse block_wait_time from config (in milliseconds, convert to Duration)
        if let Some(bwt) = doc["block_wait_time"].as_i64() {
            global.block_wait_time = Duration::from_millis(bwt as u64);
        } else if let Some(bwt) = doc["block_wait_time"].as_f64() {
            global.block_wait_time = Duration::from_millis(bwt as u64);
        }

        // Check if multi-instance mode (instances array exists)
        if let Some(instances_yaml) = doc["instances"].as_vec() {
            // Multi-instance mode
            let mut instances = Vec::new();

            for (idx, instance_yaml) in instances_yaml.iter().enumerate() {
                let mut instance = InstanceConfig::default();

                // Required: stratum_port
                if let Some(port) = instance_yaml["stratum_port"].as_str() {
                    instance.stratum_port = if port.starts_with(':') {
                        port.to_string()
                    } else if port.chars().all(|c| c.is_ascii_digit()) {
                        format!(":{}", port)
                    } else {
                        port.to_string()
                    };
                } else {
                    return Err(anyhow::anyhow!("Instance {} missing required 'stratum_port'", idx));
                }

                // Required: min_share_diff
                if let Some(diff) = instance_yaml["min_share_diff"].as_i64() {
                    instance.min_share_diff = diff as u32;
                } else {
                    return Err(anyhow::anyhow!("Instance {} missing required 'min_share_diff'", idx));
                }

                // Optional: prom_port (per-instance)
                if let Some(port) = instance_yaml["prom_port"].as_str() {
                    instance.prom_port = Some(if port.starts_with(':') {
                        port.to_string()
                    } else if port.chars().all(|c| c.is_ascii_digit()) {
                        format!(":{}", port)
                    } else {
                        port.to_string()
                    });
                }

                // Optional: log_to_file (per-instance)
                if let Some(log) = instance_yaml["log_to_file"].as_bool() {
                    instance.log_to_file = Some(log);
                }

                // Optional: instance-specific overrides
                if let Some(vd) = instance_yaml["var_diff"].as_bool() {
                    instance.var_diff = Some(vd);
                }

                if let Some(spm) = instance_yaml["shares_per_min"].as_i64() {
                    instance.shares_per_min = Some(spm as u32);
                }

                if let Some(vds) = instance_yaml["var_diff_stats"].as_bool() {
                    instance.var_diff_stats = Some(vds);
                }

                if let Some(clamp) = instance_yaml["pow2_clamp"].as_bool() {
                    instance.pow2_clamp = Some(clamp);
                }

                instances.push(instance);
            }

            if instances.is_empty() {
                return Err(anyhow::anyhow!("instances array cannot be empty"));
            }

            // Validate unique ports
            let mut ports = HashSet::new();
            for instance in &instances {
                if !ports.insert(&instance.stratum_port) {
                    return Err(anyhow::anyhow!("Duplicate stratum_port: {}", instance.stratum_port));
                }
            }

            Ok(BridgeConfig { global, instances })
        } else {
            // Single-instance mode (backward compatible)
            let mut instance = InstanceConfig::default();

            if let Some(port) = doc["stratum_port"].as_str() {
                instance.stratum_port = if port.starts_with(':') {
                    port.to_string()
                } else if port.chars().all(|c| c.is_ascii_digit()) {
                    format!(":{}", port)
                } else {
                    port.to_string()
                };
            }

            if let Some(diff) = doc["min_share_diff"].as_i64() {
                instance.min_share_diff = diff as u32;
            }

            if let Some(port) = doc["prom_port"].as_str() {
                instance.prom_port = Some(if port.starts_with(':') {
                    port.to_string()
                } else if port.chars().all(|c| c.is_ascii_digit()) {
                    format!(":{}", port)
                } else {
                    port.to_string()
                });
            }

            // Single-instance mode: use global log_to_file as instance default
            instance.log_to_file = Some(global.log_to_file);

            Ok(BridgeConfig { global, instances: vec![instance] })
        }
    }
}
