use chrono::Local;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex as StdMutex;
use tracing_subscriber::fmt::format::{FormatEvent, FormatFields, Writer};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use kaspa_stratum_bridge::log_colors::LogColors;

use crate::app_config::BridgeConfig;
use crate::app_dirs;

// Global registry mapping instance_id strings to instance numbers
// This persists across async boundaries and thread switches
// Format: "[Instance 1]" -> 1, "[Instance 2]" -> 2, etc.
static INSTANCE_REGISTRY: Lazy<StdMutex<HashMap<String, usize>>> = Lazy::new(|| StdMutex::new(HashMap::new()));

pub(crate) fn register_instance(instance_id: String, instance_num: usize) {
    if let Ok(mut registry) = INSTANCE_REGISTRY.lock() {
        registry.insert(instance_id, instance_num);
    }
}

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
        let level = *event.metadata().level();

        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S.%3f%:z");
        write!(writer, "{} ", timestamp)?;

        // Collect the message into a string first so we can analyze it for color patterns
        let mut message_buf = String::new();
        {
            let mut message_writer = Writer::new(&mut message_buf);
            ctx.format_fields(message_writer.by_ref(), event)?;
        }
        let original_message = message_buf;

        let target = event.metadata().target();
        let formatted_target =
            if let Some(rest) = target.strip_prefix("rustbridge") { format!("rustbridge{}", rest) } else { target.to_string() };
        let is_multiline = original_message.contains('\n');

        // Special-case the periodic stats output:
        // - First line rendered as: `timestamp [NODE] KSB : ...` (no [INFO], no target prefix)
        // - Table rendered below (green)
        if is_multiline && original_message.contains("| Worker") && original_message.contains("| Inst") {
            let mut lines = original_message.split('\n');
            let first_line = lines.next().unwrap_or("");
            let status_payload = first_line.strip_prefix("[NODE] ").unwrap_or(first_line);

            if self.apply_colors {
                write!(writer, "\x1b[97m[\x1b[0m\x1b[92mNODE\x1b[0m\x1b[97m]\x1b[0m KSB : {}", status_payload)?;
            } else {
                write!(writer, "[NODE] KSB : {}", status_payload)?;
            }
            writeln!(writer)?;

            for line in lines {
                let trimmed = line.trim_start();
                let is_table_line = trimmed.starts_with('+') || trimmed.starts_with('|');
                if self.apply_colors && is_table_line {
                    writeln!(writer, "\x1b[92m{}\x1b[0m", line)?;
                } else {
                    writeln!(writer, "{}", line)?;
                }
            }

            return Ok(());
        }

        // Special-case forwarded node logs (from `tracing_log::LogTracer`) to match kaspad style:
        // `[INFO] Accepted ...` (white brackets), and omit the `log:` target prefix.
        if target == "log" && !is_multiline {
            match level {
                tracing::Level::INFO => {
                    if self.apply_colors {
                        write!(writer, "\x1b[97m[\x1b[0m\x1b[92mINFO\x1b[0m\x1b[97m]\x1b[0m ")?;
                    } else {
                        write!(writer, "[INFO] ")?;
                    }
                }
                tracing::Level::WARN => {
                    if self.apply_colors {
                        write!(writer, "\x1b[97m[\x1b[0m\x1b[93mWARN\x1b[0m\x1b[97m]\x1b[0m ")?;
                    } else {
                        write!(writer, "[WARN] ")?;
                    }
                }
                tracing::Level::ERROR => {
                    if self.apply_colors {
                        write!(writer, "\x1b[97m[\x1b[0m\x1b[91mERROR\x1b[0m\x1b[97m]\x1b[0m ")?;
                    } else {
                        write!(writer, "[ERROR] ")?;
                    }
                }
                tracing::Level::DEBUG => {
                    if self.apply_colors {
                        write!(writer, "\x1b[97m[\x1b[0m\x1b[94mDEBUG\x1b[0m\x1b[97m]\x1b[0m ")?;
                    } else {
                        write!(writer, "[DEBUG] ")?;
                    }
                }
                tracing::Level::TRACE => {
                    if self.apply_colors {
                        write!(writer, "\x1b[97m[\x1b[0m\x1b[90mTRACE\x1b[0m\x1b[97m]\x1b[0m ")?;
                    } else {
                        write!(writer, "[TRACE] ")?;
                    }
                }
            }

            writeln!(writer, "{}", original_message)?;
            return Ok(());
        }

        // Default prefix: `[INFO]  target: ...` (for multiline payloads, start on a new line)
        match level {
            tracing::Level::INFO => {
                if self.apply_colors {
                    write!(writer, "\x1b[97m[\x1b[0m\x1b[92mINFO\x1b[0m\x1b[97m]\x1b[0m  ")?;
                } else {
                    write!(writer, "[INFO]  ")?;
                }
            }
            tracing::Level::WARN => {
                if self.apply_colors {
                    write!(writer, "\x1b[97m[\x1b[0m\x1b[93mWARN\x1b[0m\x1b[97m]\x1b[0m  ")?;
                } else {
                    write!(writer, "[WARN]  ")?;
                }
            }
            tracing::Level::ERROR => {
                if self.apply_colors {
                    write!(writer, "\x1b[97m[\x1b[0m\x1b[91mERROR\x1b[0m\x1b[97m]\x1b[0m  ")?;
                } else {
                    write!(writer, "[ERROR]  ")?;
                }
            }
            tracing::Level::DEBUG => {
                if self.apply_colors {
                    write!(writer, "\x1b[97m[\x1b[0m\x1b[94mDEBUG\x1b[0m\x1b[97m]\x1b[0m  ")?;
                } else {
                    write!(writer, "[DEBUG]  ")?;
                }
            }
            tracing::Level::TRACE => {
                if self.apply_colors {
                    write!(writer, "\x1b[97m[\x1b[0m\x1b[90mTRACE\x1b[0m\x1b[97m]\x1b[0m  ")?;
                } else {
                    write!(writer, "[TRACE]  ")?;
                }
            }
        }

        // Write target with capitalization. For multi-line messages (like the stats table),
        // start the payload on a new line to avoid wrapping the prefix into the table.
        let _ = is_multiline;
        write!(writer, "{}: ", formatted_target)?;

        // Check global registry for instance number based on instance_id in message
        // This works across async boundaries and thread switches
        let mut instance_num: Option<usize> = None;

        // Try to find instance_id in the message and look it up in registry
        if let Some(instance_start) = original_message.find("[Instance ")
            && let Some(instance_end) = original_message[instance_start..].find("]")
        {
            let instance_id_str = &original_message[instance_start..instance_start + instance_end + 1];
            if let Ok(registry) = INSTANCE_REGISTRY.lock()
                && let Some(&num) = registry.get(instance_id_str)
            {
                instance_num = Some(num);
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
            // Special-case the stats output (multi-line). Color the table itself green but keep the
            // preceding [NODE] lines uncolored so brackets remain white and the layout stays clean.
            if is_multiline && message.contains("| Worker") && message.contains("| Inst") {
                for line in message.split('\n') {
                    let trimmed = line.trim_start();
                    let is_table_line = trimmed.starts_with('+') || trimmed.starts_with('|');
                    if is_table_line {
                        writeln!(writer, "\x1b[92m{}\x1b[0m", line)?;
                    } else if line.contains("[NODE]") {
                        let colored = line.replace("[NODE]", "\x1b[97m[\x1b[0m\x1b[92mNODE\x1b[0m\x1b[97m]\x1b[0m");
                        writeln!(writer, "{}", colored)?;
                    } else {
                        writeln!(writer, "{}", line)?;
                    }
                }
                return Ok(());
            }

            // First priority: Use instance number from thread-local (applies to ALL logs from that instance)
            if let Some(inst_num) = instance_num {
                // Apply instance color to the entire message
                let color_code = LogColors::instance_color_code(inst_num);
                write!(writer, "{}{}\x1b[0m", color_code, &message)?;
                writeln!(writer)?;
                return Ok(());
            }

            if (message.contains("| Worker") && message.contains("| Inst")) || message.contains("| TOTAL") {
                write!(writer, "\x1b[92m{}\x1b[0m", &message)?;
                writeln!(writer)?;
                return Ok(());
            } else
            // Fallback: Check for instance pattern in message
            if let Some(instance_start) = message.find("[Instance ")
                && let Some(instance_end) = message[instance_start..].find("]")
            {
                let instance_str = &message[instance_start + 10..instance_start + instance_end];
                if let Ok(inst_num) = instance_str.parse::<usize>() {
                    // Apply instance color to the entire message
                    let color_code = LogColors::instance_color_code(inst_num);
                    write!(writer, "{}{}\x1b[0m", color_code, &message)?;
                    writeln!(writer)?;
                    return Ok(());
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
                    let label_end = message[colon_pos + 1..].chars().take_while(|c| c.is_whitespace()).count();
                    let label_end_pos = colon_pos + 1 + label_end;
                    let label = &message[..label_end_pos];
                    let value = &message[label_end_pos..];
                    write!(writer, "\x1b[94m{}\x1b[0m{}", label, value)?; // Blue for labels
                } else {
                    write!(writer, "{}", &message)?;
                }
            } else if message.contains("[NODE]") {
                let colored = message.replace("[NODE]", "\x1b[97m[\x1b[0m\x1b[92mNODE\x1b[0m\x1b[97m]\x1b[0m");
                write!(writer, "{}", colored)?;
            } else {
                write!(writer, "{}", &message)?; // No color
            }
        } else {
            write!(writer, "{}", &message)?;
        }

        writeln!(writer)
    }
}

pub(crate) fn init_tracing(
    config: &BridgeConfig,
    filter: EnvFilter,
    inprocess_mode: bool,
) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    // Setup file logging if enabled (check if any instance has logging enabled)
    // For multi-instance, we use global log_to_file setting or first instance's setting
    let should_log_to_file = config.global.log_to_file || config.instances.first().and_then(|i| i.log_to_file).unwrap_or(false);

    // Note: The file_guard must be kept alive for the lifetime of the program
    // to ensure logs are flushed to the file
    let file_guard: Option<tracing_appender::non_blocking::WorkerGuard> = if should_log_to_file {
        // Create log file with timestamp
        use std::time::SystemTime;
        let timestamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
        let log_filename = format!("rustbridge_{}.log", timestamp);
        let log_dir = app_dirs::get_bridge_logs_dir();
        let _ = std::fs::create_dir_all(&log_dir);
        let log_path = log_dir.join(&log_filename);

        // Use tracing-appender for file logging
        let file_appender = tracing_appender::rolling::never(&log_dir, &log_filename);
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        let subscriber = tracing_subscriber::registry()
            .with(filter)
            .with(
                tracing_subscriber::fmt::layer()
                    .with_ansi(LogColors::should_colorize())
                    .event_format(CustomFormatter { apply_colors: LogColors::should_colorize() }),
            )
            .with(
                tracing_subscriber::fmt::layer()
                    .with_writer(non_blocking)
                    .with_ansi(false)
                    .event_format(CustomFormatter { apply_colors: false }),
            );

        match subscriber.try_init() {
            Ok(()) => {
                eprintln!("Logging to file: {}", log_path.display());
                Some(guard)
            }
            Err(e) => {
                eprintln!("Failed to initialize tracing subscriber (already initialized?): {}", e);
                None
            }
        }
    } else {
        let subscriber = tracing_subscriber::registry().with(filter).with(
            tracing_subscriber::fmt::layer()
                .with_ansi(LogColors::should_colorize())
                .event_format(CustomFormatter { apply_colors: LogColors::should_colorize() }),
        );

        if let Err(e) = subscriber.try_init() {
            eprintln!("Failed to initialize tracing subscriber (already initialized?): {}", e);
        }

        None
    };

    // In inprocess mode, the embedded node primarily uses the `log` crate (via kaspa_core::* macros).
    // Forward those events into our tracing subscriber so users can see node startup/performance logs.
    if inprocess_mode {
        let _ = tracing_log::LogTracer::init();
    }

    file_guard
}
