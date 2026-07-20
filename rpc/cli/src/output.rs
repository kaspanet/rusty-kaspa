//! Output rendering for command results.

use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::Write as _;

/// Selects how a command result is rendered to stdout.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[clap(rename_all = "lower")]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    /// Human-readable, indented key/value listing.
    #[default]
    Text,
    /// Pretty-printed JSON.
    Json,
}

/// Serialize a typed value and render it according to the chosen format.
pub fn emit<T: serde::Serialize>(value: &T, format: OutputFormat) -> crate::error::Result<String> {
    match format {
        OutputFormat::Json => Ok(serde_json::to_string_pretty(value)?),
        OutputFormat::Text => {
            let v = serde_json::to_value(value)?;
            Ok(render(&v, format))
        }
    }
}

/// Render a JSON value according to the chosen format.
pub fn render(value: &Value, format: OutputFormat) -> String {
    match format {
        OutputFormat::Json => serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string()),
        OutputFormat::Text => {
            let mut out = String::new();
            render_text(value, 0, &mut out);
            // Trim a single trailing newline for clean printing.
            while out.ends_with('\n') {
                out.pop();
            }
            out
        }
    }
}

fn indent(level: usize, out: &mut String) {
    for _ in 0..level {
        out.push_str("  ");
    }
}

fn render_text(value: &Value, level: usize, out: &mut String) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                indent(level, out);
                if is_scalar(v) {
                    let _ = writeln!(out, "{k}: {}", scalar(v));
                } else {
                    let _ = writeln!(out, "{k}:");
                    render_text(v, level + 1, out);
                }
            }
        }
        Value::Array(items) => {
            if items.is_empty() {
                indent(level, out);
                out.push_str("[]\n");
            }
            for item in items {
                if is_scalar(item) {
                    indent(level, out);
                    let _ = writeln!(out, "- {}", scalar(item));
                } else {
                    indent(level, out);
                    out.push_str("-\n");
                    render_text(item, level + 1, out);
                }
            }
        }
        scalar => {
            indent(level, out);
            let _ = writeln!(out, "{}", self::scalar(scalar));
        }
    }
}

fn is_scalar(v: &Value) -> bool {
    !matches!(v, Value::Object(_) | Value::Array(_))
}

fn scalar(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}
