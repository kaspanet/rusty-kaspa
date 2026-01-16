use clap::{builder::BoolishValueParser, Parser, ValueEnum};
use std::path::PathBuf;
use std::time::Duration;

use crate::app_config::BridgeConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum NodeMode {
    External,
    Inprocess,
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
}

impl Cli {
    pub fn block_wait_duration(&self) -> Option<Duration> {
        self.block_wait_time.map(Duration::from_millis)
    }
}

fn normalize_port(port: &str) -> String {
    if port.starts_with(':') {
        port.to_string()
    } else if port.chars().all(|c| c.is_ascii_digit()) {
        format!(":{}", port)
    } else {
        port.to_string()
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
