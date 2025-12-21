use rustbridge::*;
use rustbridge::log_colors::LogColors;
use std::sync::Arc;
use std::time::Duration;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use yaml_rust::YamlLoader;
use futures_util::future::try_join_all;
use std::sync::Mutex as StdMutex;
use std::collections::HashMap;
use once_cell::sync::Lazy;

// Global registry mapping instance_id strings to instance numbers
// This persists across async boundaries and thread switches
// Format: "[Instance 1]" -> 1, "[Instance 2]" -> 2, etc.
static INSTANCE_REGISTRY: Lazy<StdMutex<HashMap<String, usize>>> = Lazy::new(|| {
    StdMutex::new(HashMap::new())
});

/// Instance-specific configuration
#[derive(Debug, Clone)]
struct InstanceConfig {
    stratum_port: String,
    min_share_diff: u32,
    prom_port: Option<String>,  // Optional per-instance prom port
    log_to_file: Option<bool>,  // Optional per-instance logging
    // Instance-specific settings that can override global defaults
    var_diff: Option<bool>,
    shares_per_min: Option<u32>,
    var_diff_stats: Option<bool>,
    pow2_clamp: Option<bool>,
}

/// Global configuration (shared across all instances)
#[derive(Debug, Clone)]
struct GlobalConfig {
    kaspad_address: String,
    block_wait_time: Duration,
    print_stats: bool,
    log_to_file: bool,  // Default for instances that don't specify
    health_check_port: String,
    var_diff: bool,
    shares_per_min: u32,
    var_diff_stats: bool,
    extranonce_size: u8,
    pow2_clamp: bool,
}

/// Bridge configuration (supports both single and multi-instance modes)
#[derive(Debug)]
struct BridgeConfig {
    global: GlobalConfig,
    instances: Vec<InstanceConfig>,
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

impl BridgeConfig {
    fn from_yaml(content: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let docs = YamlLoader::load_from_str(content)?;
        let doc = docs.first().ok_or("empty YAML document")?;
        
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
                    } else {
                        format!(":{}", port)
                    };
                } else {
                    return Err(format!("Instance {} missing required 'stratum_port'", idx).into());
                }
                
                // Required: min_share_diff
                if let Some(diff) = instance_yaml["min_share_diff"].as_i64() {
                    instance.min_share_diff = diff as u32;
                } else {
                    return Err(format!("Instance {} missing required 'min_share_diff'", idx).into());
                }
                
                // Optional: prom_port (per-instance)
                if let Some(port) = instance_yaml["prom_port"].as_str() {
                    instance.prom_port = Some(if port.starts_with(':') {
                        port.to_string()
                    } else {
                        format!(":{}", port)
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
                return Err("instances array cannot be empty".into());
            }
            
            // Validate unique ports
            let mut ports = std::collections::HashSet::new();
            for instance in &instances {
                if !ports.insert(&instance.stratum_port) {
                    return Err(format!("Duplicate stratum_port: {}", instance.stratum_port).into());
                }
            }
            
            Ok(BridgeConfig { global, instances })
        } else {
            // Single-instance mode (backward compatible)
            let mut instance = InstanceConfig::default();
            
            if let Some(port) = doc["stratum_port"].as_str() {
                instance.stratum_port = if port.starts_with(':') {
                    port.to_string()
                } else {
                    format!(":{}", port)
                };
            }
            
            if let Some(diff) = doc["min_share_diff"].as_i64() {
                instance.min_share_diff = diff as u32;
            }
            
            if let Some(port) = doc["prom_port"].as_str() {
                instance.prom_port = Some(if port.starts_with(':') {
                    port.to_string()
                } else {
                    format!(":{}", port)
                });
            }
            
            // Single-instance mode: use global log_to_file as instance default
            instance.log_to_file = Some(global.log_to_file);
            
            Ok(BridgeConfig {
                global,
                instances: vec![instance],
            })
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load config first to check if file logging is enabled
    let config_path = std::path::Path::new("config.yaml");
    let config = if config_path.exists() {
        let content = std::fs::read_to_string(config_path)?;
        BridgeConfig::from_yaml(&content)?
    } else {
        // Create default single-instance config
        BridgeConfig {
            global: GlobalConfig::default(),
            instances: vec![InstanceConfig::default()],
        }
    };
    
    // Initialize color support detection
    rustbridge::log_colors::LogColors::init();
    
    // Initialize tracing with WARN level by default (less verbose)
    // Can be overridden with RUST_LOG environment variable (e.g., RUST_LOG=info,debug)
    // To see more details, set RUST_LOG=info or RUST_LOG=debug
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| {
            // Default: warn level, but allow info from rustbridge module for important messages
            EnvFilter::new("warn,rustbridge=info")
        });
    
    // Custom formatter that applies colors directly to the Writer (like tracing-subscriber does for levels)
    // We create two formatters: one with colors (for console) and one without (for file)
    use tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
    use std::fmt;
    
    struct CustomFormatter {
        apply_colors: bool,
    }
    
    impl<S, N> FormatEvent<S, N> for CustomFormatter
    where
        S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
        N: for<'a> FormatFields<'a> + 'static,
    {
        fn format_event(
            &self,
            ctx: &tracing_subscriber::fmt::FmtContext<'_, S, N>,
            mut writer: Writer<'_>,
            event: &tracing::Event<'_>,
        ) -> fmt::Result {
            // Write level (with built-in ANSI colors from tracing-subscriber)
            let level = *event.metadata().level();
            write!(writer, "{:5} ", level)?;
            
            // Write target with capitalization
            let target = event.metadata().target();
            let formatted_target = if target.starts_with("rustbridge") {
                format!("rustbridge{}", target.strip_prefix("rustbridge").unwrap_or(target))
            } else {
                target.to_string()
            };
            write!(writer, "{}: ", formatted_target)?;
            
            // Collect the message into a string first so we can analyze it for color patterns
            let mut message_buf = String::new();
            {
                let mut message_writer = Writer::new(&mut message_buf);
                ctx.format_fields(message_writer.by_ref(), event)?;
            }
            let original_message = message_buf;
            
            // Check global registry for instance number based on instance_id in message
            // This works across async boundaries and thread switches
            let mut instance_num: Option<usize> = None;
            
            // Try to find instance_id in the message and look it up in registry
            if let Some(instance_start) = original_message.find("[Instance ") {
                if let Some(instance_end) = original_message[instance_start..].find("]") {
                    let instance_id_str = &original_message[instance_start..instance_start + instance_end + 1];
                    if let Ok(registry) = INSTANCE_REGISTRY.lock() {
                        if let Some(&num) = registry.get(instance_id_str) {
                            instance_num = Some(num);
                        }
                    }
                }
            }
            
            // Check if message already contains colored instance identifier
            // If it does, preserve it and write as-is (don't strip ANSI codes)
            let has_colored_instance = original_message.contains("\x1b[") && original_message.contains("[Instance ");
            
            if has_colored_instance && self.apply_colors {
                // Message already has instance colors, write it as-is
                write!(writer, "{}", original_message)?;
                writeln!(writer)?;
                return Ok(());
            }
            
            // Strip any existing ANSI codes from the message for pattern matching
            let mut cleaned_message = String::new();
            let mut chars = original_message.chars().peekable();
            while let Some(ch) = chars.next() {
                if ch == '\x1b' {
                    // Skip ANSI escape sequence: \x1b[ followed by numbers and letters until 'm'
                    if chars.peek() == Some(&'[') {
                        chars.next(); // consume '['
                        while let Some(&c) = chars.peek() {
                            if c == 'm' {
                                chars.next(); // consume 'm'
                                break;
                            }
                            chars.next();
                        }
                    }
                } else {
                    cleaned_message.push(ch);
                }
            }
            let message = cleaned_message;
            
            // Apply colors based on message content patterns (only if this formatter has colors enabled)
            if self.apply_colors {
                // First priority: Use instance number from thread-local (applies to ALL logs from that instance)
                if let Some(inst_num) = instance_num {
                    // Apply instance color to the entire message
                    let color_code = rustbridge::log_colors::LogColors::instance_color_code(inst_num);
                    write!(writer, "{}{}\x1b[0m", color_code, &message)?;
                    writeln!(writer)?;
                    return Ok(());
                }
                
                // Fallback: Check for instance pattern in message
                if let Some(instance_start) = message.find("[Instance ") {
                    if let Some(instance_end) = message[instance_start..].find("]") {
                        let instance_str = &message[instance_start + 10..instance_start + instance_end];
                        if let Ok(inst_num) = instance_str.parse::<usize>() {
                            // Apply instance color to the entire message
                            let color_code = rustbridge::log_colors::LogColors::instance_color_code(inst_num);
                            write!(writer, "{}{}\x1b[0m", color_code, &message)?;
                            writeln!(writer)?;
                            return Ok(());
                        }
                    }
                }
                if message.contains("[ASIC->BRIDGE]") {
                    write!(writer, "\x1b[96m{}\x1b[0m", &message)?; // Cyan
                } else if message.contains("[BRIDGE->ASIC]") {
                    write!(writer, "\x1b[92m{}\x1b[0m", &message)?; // Green
                } else if message.contains("[VALIDATION]") {
                    write!(writer, "\x1b[93m{}\x1b[0m", &message)?; // Yellow
                } else if message.contains("===== BLOCK") || message.contains("[BLOCK]") {
                    write!(writer, "\x1b[95m{}\x1b[0m", &message)?; // Magenta
                } else if message.contains("[API]") {
                    write!(writer, "\x1b[94m{}\x1b[0m", &message)?; // Blue
                } else if message.contains("Error") || message.contains("ERROR") {
                    write!(writer, "\x1b[91m{}\x1b[0m", &message)?; // Red
                } else if message.contains("----------------------------------") {
                    write!(writer, "\x1b[96m{}\x1b[0m", &message)?; // Bright Cyan for separator lines
                } else if message.contains("initializing bridge") {
                    write!(writer, "\x1b[92m{}\x1b[0m", &message)?; // Bright Green for initialization
                } else if message.contains("Starting RustBridge") {
                    write!(writer, "\x1b[92m{}\x1b[0m", &message)?; // Bright Green for startup
                } else if message.starts_with("\t") && message.contains(":") {
                    // Configuration lines - color the label part (e.g., "\tkaspad:          value")
                    if let Some(colon_pos) = message.find(':') {
                        // Find the end of the label (colon + whitespace)
                        let label_end = message[colon_pos + 1..]
                            .chars()
                            .take_while(|c| c.is_whitespace())
                            .count();
                        let label_end_pos = colon_pos + 1 + label_end;
                        let label = &message[..label_end_pos];
                        let value = &message[label_end_pos..];
                        write!(writer, "\x1b[94m{}\x1b[0m{}", label, value)?; // Blue for labels
                    } else {
                        write!(writer, "{}", &message)?;
                    }
                } else {
                    write!(writer, "{}", &message)?; // No color
                }
            } else {
                write!(writer, "{}", &message)?;
            }
            
            writeln!(writer)
        }
    }
    
    // Setup file logging if enabled (check if any instance has logging enabled)
    // For multi-instance, we use global log_to_file setting or first instance's setting
    let should_log_to_file = config.global.log_to_file || 
        config.instances.first().and_then(|i| i.log_to_file).unwrap_or(false);
    
    // Note: The file_guard must be kept alive for the lifetime of the program
    // to ensure logs are flushed to the file
    let _file_guard: Option<tracing_appender::non_blocking::WorkerGuard> = if should_log_to_file {
        // Create log file with timestamp
        use std::time::SystemTime;
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let log_filename = format!("rustbridge_{}.log", timestamp);
        let log_path = std::path::Path::new(".").join(&log_filename);
        
        // Use tracing-appender for file logging
        let file_appender = tracing_appender::rolling::never(".", &log_filename);
        let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
        
        eprintln!("Logging to file: {}", log_path.display());
        
        // Setup logging with both console and file
        // Use default formatter for console (preserves ANSI codes) but with custom target formatting
        tracing_subscriber::registry()
            .with(filter)
            .with(
                tracing_subscriber::fmt::layer()
                    .with_ansi(LogColors::should_colorize()) // Enable ANSI colors for console conditionally
                    .event_format(CustomFormatter { apply_colors: LogColors::should_colorize() })
            )
            .with(
                tracing_subscriber::fmt::layer()
                    .with_writer(non_blocking)
                    .with_ansi(false) // Disable ANSI colors in file
                    .event_format(CustomFormatter { apply_colors: false })
            )
            .init();
        
        Some(_guard)
    } else {
        // Setup logging with console only
        tracing_subscriber::registry()
            .with(filter)
            .with(
                tracing_subscriber::fmt::layer()
                    .with_ansi(LogColors::should_colorize()) // Enable ANSI colors for console conditionally
                    .event_format(CustomFormatter { apply_colors: LogColors::should_colorize() })
                    // Use default formatter to preserve ANSI codes in messages
                    // .event_format(CustomFormatter)
            )
            .init();
        
        None
    };
    
    if !config_path.exists() {
        tracing::warn!("config.yaml not found, using defaults");
    }
    
    let instance_count = config.instances.len();
    tracing::info!("----------------------------------");
    tracing::info!("initializing bridge ({} instance{})", instance_count, if instance_count > 1 { "s" } else { "" });
    tracing::info!("\tkaspad:          {} (shared)", config.global.kaspad_address);
    tracing::info!("\tblock wait:      {:?}", config.global.block_wait_time);
    tracing::info!("\tprint stats:     {}", config.global.print_stats);
    tracing::info!("\tvar diff:        {}", config.global.var_diff);
    tracing::info!("\tshares per min:  {}", config.global.shares_per_min);
    tracing::info!("\tvar diff stats:  {}", config.global.var_diff_stats);
    tracing::info!("\tpow2 clamp:      {}", config.global.pow2_clamp);
    tracing::info!("\textranonce:      auto-detected per client");
    tracing::info!("\thealth check:    {}", config.global.health_check_port);
    
    for (idx, instance) in config.instances.iter().enumerate() {
        tracing::info!("\t--- Instance {} ---", idx + 1);
        tracing::info!("\t  stratum:       {}", instance.stratum_port);
        tracing::info!("\t  min diff:      {}", instance.min_share_diff);
        if let Some(ref prom_port) = instance.prom_port {
            tracing::info!("\t  prom:          {}", prom_port);
        }
        if let Some(log_to_file) = instance.log_to_file {
            tracing::info!("\t  log to file:   {}", log_to_file);
        }
    }
    tracing::info!("----------------------------------");
    
    // Start global health check server if port is specified
    if !config.global.health_check_port.is_empty() {
        let health_port = config.global.health_check_port.clone();
        tokio::spawn(async move {
            use tokio::net::TcpListener;
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            
            if let Ok(listener) = TcpListener::bind(&health_port).await {
                tracing::info!("Health check server started on {}", health_port);
                loop {
                    if let Ok((mut stream, _)) = listener.accept().await {
                        let mut buffer = [0; 1024];
                        if stream.read(&mut buffer).await.is_ok() {
                            let response = "HTTP/1.1 200 OK\r\n\r\n";
                            let _ = stream.write_all(response.as_bytes()).await;
                        }
                    }
                }
            }
        });
    }
    
    // Create shared kaspa API client (all instances use the same node)
    let kaspa_api = rustbridge::KaspaApi::new(
        config.global.kaspad_address.clone(),
        config.global.block_wait_time,
    ).await.map_err(|e| format!("Failed to create Kaspa API client: {}", e))?;

    // Spawn each instance
    let mut instance_handles = Vec::new();
    
    for (idx, instance_config) in config.instances.iter().enumerate() {
        let instance_num = idx + 1;
        let instance = instance_config.clone();
        let global = config.global.clone();
        let kaspa_api_clone = Arc::clone(&kaspa_api);
        
        // Only the first instance should try to use notification-based listener
        // All other instances will use polling (notification receiver can only be taken once)
        let is_first_instance = idx == 0;
        
        // Spawn instance-specific Prometheus server if configured
        if let Some(ref prom_port) = instance.prom_port {
            let prom_port = prom_port.clone();
            let instance_num_prom = instance_num;
            tokio::spawn(async move {
                if let Err(e) = prom::start_prom_server(&prom_port).await {
                    tracing::error!("[Instance {}] Prometheus server error: {}", instance_num_prom, e);
                }
            });
        }
        
        // Spawn this instance
        let handle = tokio::spawn(async move {
            // Register this instance in the global registry - this persists across async boundaries
            let instance_id_str = rustbridge::log_colors::LogColors::format_instance_id(instance_num);
            {
                if let Ok(mut registry) = INSTANCE_REGISTRY.lock() {
                    registry.insert(instance_id_str.clone(), instance_num);
                }
            }
            
            // Use colored instance identifier for startup message
            let colored_instance_id = rustbridge::log_colors::LogColors::format_instance_id(instance_num);
            tracing::info!("{} Starting on stratum port {}", colored_instance_id, instance.stratum_port);
            
            // Create bridge config for this instance
            // Store the instance_id for use in logs
            let bridge_config = rustbridge::BridgeConfig {
                instance_id: instance_id_str.clone(),
                stratum_port: instance.stratum_port.clone(),
                kaspad_address: global.kaspad_address.clone(),
                prom_port: String::new(), // Prometheus handled separately per-instance
                print_stats: global.print_stats,
                log_to_file: instance.log_to_file.unwrap_or(global.log_to_file),
                health_check_port: String::new(), // Global health check only
                block_wait_time: global.block_wait_time,
                min_share_diff: instance.min_share_diff,
                var_diff: instance.var_diff.unwrap_or(global.var_diff),
                shares_per_min: instance.shares_per_min.unwrap_or(global.shares_per_min),
                var_diff_stats: instance.var_diff_stats.unwrap_or(global.var_diff_stats),
                extranonce_size: global.extranonce_size,
                pow2_clamp: instance.pow2_clamp.unwrap_or(global.pow2_clamp),
            };
            
            // Start this instance
            // Only first instance gets concrete_api (for notifications), others use None (polling only)
            rustbridge::listen_and_serve(
                bridge_config, 
                Arc::clone(&kaspa_api_clone), 
                if is_first_instance { Some(kaspa_api_clone) } else { None }
            ).await
            .map_err(|e| format!("[Instance {}] Bridge server error: {}", instance_num, e))
        });
        
        instance_handles.push(handle);
    }
    
    // Wait for all instances (if any fails, we'll know)
    tracing::info!("All {} instance(s) started, waiting for completion...", instance_count);
    
    // Wait for all instances (if any fails, we'll know)
    let result = try_join_all(instance_handles).await;
    
    match result {
        Ok(_) => {
            tracing::info!("All instances completed successfully");
            Ok(())
        }
        Err(e) => {
            tracing::error!("One or more instances failed: {:?}", e);
            Err(format!("Instance error: {:?}", e).into())
        }
    }
}

