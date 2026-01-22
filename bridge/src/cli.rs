use clap::{Parser, ValueEnum, builder::BoolishValueParser};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use crate::app_config::BridgeConfig;
use crate::app_config::InstanceConfig;
use kaspa_stratum_bridge::net_utils::normalize_port;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum NodeMode {
    External,
    Inprocess,
}

fn parse_bool(s: &str) -> Result<bool, anyhow::Error> {
    match s.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "y" | "on" => Ok(true),
        "false" | "0" | "no" | "n" | "off" => Ok(false),
        _ => Err(anyhow::anyhow!("invalid boolean value: {s}")),
    }
}

fn parse_instance_spec(spec: &str, default_min_share_diff: Option<u32>) -> Result<InstanceConfig, anyhow::Error> {
    let mut instance = InstanceConfig { stratum_port: String::new(), min_share_diff: 0, ..InstanceConfig::default() };

    let mut has_port = false;
    let mut has_diff = false;

    for raw_part in spec.split(',') {
        let part = raw_part.trim();
        if part.is_empty() {
            continue;
        }

        let (k, v) = match part.split_once('=') {
            Some((k, v)) => (k.trim(), v.trim()),
            None => ("port", part),
        };

        match k.to_ascii_lowercase().as_str() {
            "port" | "stratum" | "stratum_port" => {
                instance.stratum_port = normalize_port(v);
                has_port = true;
            }
            "prom" | "prom_port" => {
                instance.prom_port = Some(normalize_port(v));
            }
            "diff" | "min_share_diff" => {
                instance.min_share_diff = v.parse::<u32>().map_err(|e| anyhow::anyhow!("invalid min_share_diff '{v}': {e}"))?;
                has_diff = true;
            }
            "wait" | "block_wait_time" => {
                let ms = v.parse::<u64>().map_err(|e| anyhow::anyhow!("invalid block_wait_time '{v}': {e}"))?;
                instance.block_wait_time = Some(Duration::from_millis(ms));
            }
            "extranonce" | "extranonce_size" => {
                instance.extranonce_size = Some(v.parse::<u8>().map_err(|e| anyhow::anyhow!("invalid extranonce_size '{v}': {e}"))?);
            }
            "log" | "log_to_file" => {
                instance.log_to_file = Some(parse_bool(v)?);
            }
            "var_diff" => {
                instance.var_diff = Some(parse_bool(v)?);
            }
            "shares_per_min" => {
                instance.shares_per_min = Some(v.parse::<u32>().map_err(|e| anyhow::anyhow!("invalid shares_per_min '{v}': {e}"))?);
            }
            "var_diff_stats" => {
                instance.var_diff_stats = Some(parse_bool(v)?);
            }
            "pow2_clamp" => {
                instance.pow2_clamp = Some(parse_bool(v)?);
            }
            _ => {
                return Err(anyhow::anyhow!("unknown instance key '{k}' in '{spec}'"));
            }
        }
    }

    if !has_port {
        return Err(anyhow::anyhow!("instance is missing required 'port' in '{spec}'"));
    }

    if !has_diff {
        if let Some(d) = default_min_share_diff {
            instance.min_share_diff = d;
        } else {
            return Err(anyhow::anyhow!(
                "instance is missing required 'diff' (min_share_diff) in '{spec}' and no global --min-share-diff was provided"
            ));
        }
    }

    Ok(instance)
}

#[derive(Debug, Clone, Parser)]
#[command(author, version, about)]
pub struct Cli {
    #[arg(long)]
    pub config: Option<PathBuf>,

    #[arg(long)]
    pub testnet: bool,

    #[arg(long, value_enum)]
    pub node_mode: Option<NodeMode>,

    #[arg(long)]
    pub appdir: Option<PathBuf>,

    #[arg(last = true, help = "Kaspad arguments (use '--' separator if kaspad args start with hyphens)")]
    pub kaspad_args: Vec<String>,

    #[arg(long)]
    pub kaspad_address: Option<String>,

    #[arg(long)]
    pub block_wait_time: Option<u64>,

    #[arg(long, value_parser = BoolishValueParser::new())]
    pub print_stats: Option<bool>,

    #[arg(long, value_parser = BoolishValueParser::new())]
    pub log_to_file: Option<bool>,

    #[arg(long)]
    pub health_check_port: Option<String>,

    /// Global Web UI / aggregated metrics server port (optional). Examples: ":3030", "0.0.0.0:3030"
    #[arg(long)]
    pub web_port: Option<String>,

    #[arg(long, value_parser = BoolishValueParser::new())]
    pub var_diff: Option<bool>,

    #[arg(long)]
    pub shares_per_min: Option<u32>,

    #[arg(long, value_parser = BoolishValueParser::new())]
    pub var_diff_stats: Option<bool>,

    #[arg(long)]
    pub extranonce_size: Option<u8>,

    #[arg(long, value_parser = BoolishValueParser::new())]
    pub pow2_clamp: Option<bool>,

    #[arg(long)]
    pub coinbase_tag_suffix: Option<String>,

    #[arg(long)]
    pub stratum_port: Option<String>,

    #[arg(long)]
    pub min_share_diff: Option<u32>,

    #[arg(long)]
    pub prom_port: Option<String>,

    #[arg(long = "instance")]
    pub instances: Vec<String>,

    #[arg(long, value_parser = BoolishValueParser::new())]
    pub instance_log_to_file: Option<bool>,

    #[arg(long, value_parser = BoolishValueParser::new())]
    pub instance_var_diff: Option<bool>,

    #[arg(long)]
    pub instance_shares_per_min: Option<u32>,

    #[arg(long, value_parser = BoolishValueParser::new())]
    pub instance_var_diff_stats: Option<bool>,

    #[arg(long, value_parser = BoolishValueParser::new())]
    pub instance_pow2_clamp: Option<bool>,

    // ---------------------------
    // Internal CPU miner (feature-gated)
    // ---------------------------
    /// Enable the built-in CPU miner (solo mining). Requires `--internal-cpu-miner-address`.
    #[cfg(feature = "rkstratum_cpu_miner")]
    #[arg(long, default_value_t = false)]
    pub internal_cpu_miner: bool,

    /// Mining address (reward address) used by the internal CPU miner.
    #[cfg(feature = "rkstratum_cpu_miner")]
    #[arg(long)]
    pub internal_cpu_miner_address: Option<String>,

    /// Number of CPU mining threads.
    #[cfg(feature = "rkstratum_cpu_miner")]
    #[arg(long)]
    pub internal_cpu_miner_threads: Option<usize>,

    /// Optional per-hash sleep (throttle) in milliseconds.
    #[cfg(feature = "rkstratum_cpu_miner")]
    #[arg(long)]
    pub internal_cpu_miner_throttle_ms: Option<u64>,

    /// Block template poll interval in milliseconds (how often to refresh work).
    #[cfg(feature = "rkstratum_cpu_miner")]
    #[arg(long)]
    pub internal_cpu_miner_template_poll_ms: Option<u64>,
}

impl Cli {
    pub fn block_wait_duration(&self) -> Option<Duration> {
        self.block_wait_time.map(Duration::from_millis)
    }
}

pub fn apply_cli_overrides(config: &mut BridgeConfig, cli: &Cli) -> Result<(), anyhow::Error> {
    if let Some(addr) = cli.kaspad_address.as_deref() {
        config.global.kaspad_address = addr.to_string();
    }
    if let Some(dur) = cli.block_wait_duration() {
        config.global.block_wait_time = dur;
    }
    if let Some(v) = cli.print_stats {
        config.global.print_stats = v;
    }
    if let Some(v) = cli.log_to_file {
        config.global.log_to_file = v;
    }
    if let Some(port) = cli.health_check_port.as_deref() {
        config.global.health_check_port = port.to_string();
    }
    if let Some(port) = cli.web_port.as_deref() {
        config.global.web_port = normalize_port(port);
    }
    if let Some(v) = cli.var_diff {
        config.global.var_diff = v;
    }
    if let Some(v) = cli.shares_per_min {
        config.global.shares_per_min = v;
    }
    if let Some(v) = cli.var_diff_stats {
        config.global.var_diff_stats = v;
    }
    if let Some(v) = cli.extranonce_size {
        config.global.extranonce_size = v;
    }
    if let Some(v) = cli.pow2_clamp {
        config.global.pow2_clamp = v;
    }
    if let Some(s) = cli.coinbase_tag_suffix.as_deref() {
        let s = s.trim();
        config.global.coinbase_tag_suffix = if s.is_empty() { None } else { Some(s.to_string()) };
    }

    if !cli.instances.is_empty() {
        if cli.stratum_port.is_some()
            || cli.prom_port.is_some()
            || cli.instance_log_to_file.is_some()
            || cli.instance_var_diff.is_some()
            || cli.instance_shares_per_min.is_some()
            || cli.instance_var_diff_stats.is_some()
            || cli.instance_pow2_clamp.is_some()
        {
            return Err(anyhow::anyhow!(
                "when using --instance, do not use the single-instance flags --stratum-port/--prom-port/--instance-*; put per-instance overrides inside the --instance spec"
            ));
        }

        let mut instances = Vec::with_capacity(cli.instances.len());
        for spec in &cli.instances {
            instances.push(parse_instance_spec(spec, cli.min_share_diff)?);
        }

        if instances.is_empty() {
            return Err(anyhow::anyhow!("at least one --instance is required"));
        }

        let mut ports = HashSet::new();
        for instance in &instances {
            if !ports.insert(instance.stratum_port.as_str()) {
                return Err(anyhow::anyhow!("duplicate stratum port: {}", instance.stratum_port));
            }
        }

        config.instances = instances;
        return Ok(());
    }

    let mut has_instance_overrides = false;
    has_instance_overrides |= cli.stratum_port.is_some();
    has_instance_overrides |= cli.min_share_diff.is_some();
    has_instance_overrides |= cli.prom_port.is_some();
    has_instance_overrides |= cli.instance_log_to_file.is_some();
    has_instance_overrides |= cli.instance_var_diff.is_some();
    has_instance_overrides |= cli.instance_shares_per_min.is_some();
    has_instance_overrides |= cli.instance_var_diff_stats.is_some();
    has_instance_overrides |= cli.instance_pow2_clamp.is_some();

    if has_instance_overrides && config.instances.len() != 1 {
        return Err(anyhow::anyhow!("instance-specific CLI overrides are only supported when exactly one instance is configured"));
    }

    if config.instances.len() == 1 {
        let instance = &mut config.instances[0];

        if let Some(port) = cli.stratum_port.as_deref() {
            instance.stratum_port = normalize_port(port);
        }
        if let Some(diff) = cli.min_share_diff {
            instance.min_share_diff = diff;
        }
        if let Some(port) = cli.prom_port.as_deref() {
            instance.prom_port = Some(normalize_port(port));
        }
        if let Some(v) = cli.instance_log_to_file {
            instance.log_to_file = Some(v);
        }
        if let Some(v) = cli.instance_var_diff {
            instance.var_diff = Some(v);
        }
        if let Some(v) = cli.instance_shares_per_min {
            instance.shares_per_min = Some(v);
        }
        if let Some(v) = cli.instance_var_diff_stats {
            instance.var_diff_stats = Some(v);
        }
        if let Some(v) = cli.instance_pow2_clamp {
            instance.pow2_clamp = Some(v);
        }
    }

    Ok(())
}
