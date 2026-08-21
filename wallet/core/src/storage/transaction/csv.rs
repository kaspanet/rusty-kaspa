//!
//! CSV export utilities for transaction records.
//!

use super::{TransactionData, TransactionRecord, UtxoRecord};
use crate::utils::sompi_to_kaspa_string_with_trailing_zeroes;

/// Escapes a string value for CSV format (RFC 4180 compliant).
/// 
/// Fields containing commas, double quotes, or newlines are enclosed in double quotes.
/// Double quotes within fields are escaped by doubling them.
pub fn escape_csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// CSV header row for transaction export.
pub const CSV_HEADER: &str = "transaction_id,timestamp,timestamp_unix_ms,block_daa_score,transaction_type,amount,amount_sompi,fees_sompi,fees,is_coinbase,network,note,utxo_count,addresses";

/// A flat CSV-friendly representation of a transaction record.
#[derive(Debug, Clone)]
pub struct TransactionCsvRecord {
    pub transaction_id: String,
    pub timestamp: String,
    pub timestamp_unix_ms: Option<u64>,
    pub block_daa_score: u64,
    pub transaction_type: String,
    pub amount: String,
    pub amount_sompi: i64,
    pub fees_sompi: Option<u64>,
    pub fees: Option<String>,
    pub is_coinbase: bool,
    pub network: String,
    pub note: Option<String>,
    pub utxo_count: usize,
    pub addresses: String,
}

impl TransactionCsvRecord {
    /// Convert to a CSV row string (without trailing newline).
    pub fn to_csv_row(&self) -> String {
        format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            self.transaction_id,
            self.timestamp,
            self.timestamp_unix_ms.map(|t| t.to_string()).unwrap_or_default(),
            self.block_daa_score,
            self.transaction_type,
            self.amount,
            self.amount_sompi,
            self.fees_sompi.map(|f| f.to_string()).unwrap_or_default(),
            self.fees.as_deref().unwrap_or(""),
            self.is_coinbase,
            self.network,
            escape_csv_field(self.note.as_deref().unwrap_or("")),
            self.utxo_count,
            escape_csv_field(&self.addresses),
        )
    }
}

impl TransactionRecord {
    /// Convert this transaction record to a CSV-compatible flat structure.
    pub fn to_csv_record(&self) -> TransactionCsvRecord {
        let timestamp = self.format_timestamp_iso8601();
        let transaction_type = self.transaction_data.kind().to_string();
        
        let (amount_sompi, fees_sompi, is_coinbase, utxo_count, addresses) = 
            self.extract_csv_fields();
        
        let amount = self.format_amount_for_csv(amount_sompi);
        let fees = fees_sompi.map(sompi_to_kaspa_string_with_trailing_zeroes);
        
        TransactionCsvRecord {
            transaction_id: self.id.to_string(),
            timestamp,
            timestamp_unix_ms: self.unixtime_msec,
            block_daa_score: self.block_daa_score,
            transaction_type,
            amount,
            amount_sompi,
            fees_sompi,
            fees,
            is_coinbase,
            network: self.network_id.to_string(),
            note: self.note.clone(),
            utxo_count,
            addresses,
        }
    }

    /// Generate a single CSV row string (without header or trailing newline).
    pub fn to_csv_row(&self) -> String {
        self.to_csv_record().to_csv_row()
    }

    /// Returns the CSV header row.
    pub fn csv_header() -> &'static str {
        CSV_HEADER
    }

    /// Format timestamp as ISO 8601 string.
    fn format_timestamp_iso8601(&self) -> String {
        self.unixtime_msec
            .map(|ms| {
                // Format as ISO 8601: YYYY-MM-DDTHH:MM:SS.sssZ
                let secs = (ms / 1000) as i64;
                let millis = (ms % 1000) as u32;
                
                // Calculate date/time components from unix timestamp
                let days_since_epoch = secs / 86400;
                let time_of_day = secs % 86400;
                
                let hours = time_of_day / 3600;
                let minutes = (time_of_day % 3600) / 60;
                let seconds = time_of_day % 60;
                
                // Calculate year, month, day from days since epoch (1970-01-01)
                let (year, month, day) = days_to_ymd(days_since_epoch);
                
                format!(
                    "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
                    year, month, day, hours, minutes, seconds, millis
                )
            })
            .unwrap_or_default()
    }

    /// Format amount for CSV display (with sign for outgoing).
    fn format_amount_for_csv(&self, amount_sompi: i64) -> String {
        let abs_amount = amount_sompi.unsigned_abs();
        let formatted = sompi_to_kaspa_string_with_trailing_zeroes(abs_amount);
        if amount_sompi < 0 {
            format!("-{}", formatted)
        } else {
            formatted
        }
    }

    /// Extract CSV-specific fields from transaction data.
    fn extract_csv_fields(&self) -> (i64, Option<u64>, bool, usize, String) {
        match &self.transaction_data {
            TransactionData::Incoming { utxo_entries, aggregate_input_value }
            | TransactionData::Stasis { utxo_entries, aggregate_input_value }
            | TransactionData::Reorg { utxo_entries, aggregate_input_value } => {
                let is_coinbase = utxo_entries.iter().any(|e| e.is_coinbase);
                let addresses = extract_addresses(utxo_entries);
                (*aggregate_input_value as i64, None, is_coinbase, utxo_entries.len(), addresses)
            }
            TransactionData::External { utxo_entries, aggregate_input_value } => {
                let addresses = extract_addresses(utxo_entries);
                // External transactions are outgoing (negative)
                (-(*aggregate_input_value as i64), None, false, utxo_entries.len(), addresses)
            }
            TransactionData::Outgoing { fees, aggregate_input_value, payment_value, utxo_entries, .. } => {
                let value = payment_value.unwrap_or(*aggregate_input_value);
                let addresses = extract_addresses(utxo_entries);
                (-(value as i64), Some(*fees), false, utxo_entries.len(), addresses)
            }
            TransactionData::Batch { fees, aggregate_input_value, utxo_entries, .. } => {
                let addresses = extract_addresses(utxo_entries);
                // Batch transactions are internal, show as neutral/negative
                (-(*aggregate_input_value as i64), Some(*fees), false, utxo_entries.len(), addresses)
            }
            TransactionData::TransferIncoming { fees, payment_value, aggregate_input_value, utxo_entries, .. } => {
                let value = payment_value.unwrap_or(*aggregate_input_value);
                let addresses = extract_addresses(utxo_entries);
                (value as i64, Some(*fees), false, utxo_entries.len(), addresses)
            }
            TransactionData::TransferOutgoing { fees, payment_value, aggregate_input_value, utxo_entries, .. } => {
                let value = payment_value.unwrap_or(*aggregate_input_value);
                let addresses = extract_addresses(utxo_entries);
                (-(value as i64), Some(*fees), false, utxo_entries.len(), addresses)
            }
            TransactionData::Change { aggregate_input_value, utxo_entries, .. } => {
                let addresses = extract_addresses(utxo_entries);
                (*aggregate_input_value as i64, None, false, utxo_entries.len(), addresses)
            }
        }
    }
}

/// Extract addresses from UTXO entries as a comma-separated string.
fn extract_addresses(utxo_entries: &[UtxoRecord]) -> String {
    utxo_entries
        .iter()
        .filter_map(|e| e.address.as_ref().map(|a| a.to_string()))
        .collect::<Vec<_>>()
        .join(",")
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_ymd(days: i64) -> (i32, u32, u32) {
    // Algorithm based on Howard Hinnant's date algorithms
    // http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (year as i32, m, d)
}

/// Generate complete CSV content from a list of transaction records.
/// 
/// Returns a string containing the CSV header followed by one row per transaction.
pub fn generate_csv(records: &[TransactionRecord]) -> String {
    let mut csv = String::from(CSV_HEADER);
    csv.push('\n');
    for record in records {
        csv.push_str(&record.to_csv_row());
        csv.push('\n');
    }
    csv
}

/// Generate CSV content from an iterator of transaction records.
/// 
/// This is more memory-efficient for large transaction histories.
pub fn generate_csv_from_iter<'a, I>(records: I) -> String 
where
    I: Iterator<Item = &'a TransactionRecord>,
{
    let mut csv = String::from(CSV_HEADER);
    csv.push('\n');
    for record in records {
        csv.push_str(&record.to_csv_row());
        csv.push('\n');
    }
    csv
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_csv_escape_simple() {
        assert_eq!(escape_csv_field("hello"), "hello");
        assert_eq!(escape_csv_field("hello world"), "hello world");
    }

    #[test]
    fn test_csv_escape_comma() {
        assert_eq!(escape_csv_field("hello,world"), "\"hello,world\"");
    }

    #[test]
    fn test_csv_escape_quotes() {
        assert_eq!(escape_csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn test_csv_escape_newline() {
        assert_eq!(escape_csv_field("line1\nline2"), "\"line1\nline2\"");
    }

    #[test]
    fn test_csv_escape_carriage_return() {
        assert_eq!(escape_csv_field("line1\r\nline2"), "\"line1\r\nline2\"");
    }

    #[test]
    fn test_csv_escape_empty() {
        assert_eq!(escape_csv_field(""), "");
    }

    #[test]
    fn test_csv_escape_complex() {
        assert_eq!(
            escape_csv_field("Payment for \"widgets\", qty: 5"),
            "\"Payment for \"\"widgets\"\", qty: 5\""
        );
    }

    #[test]
    fn test_days_to_ymd_epoch() {
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
    }

    #[test]
    fn test_days_to_ymd_2024() {
        // 2024-01-01 is 19723 days since epoch
        assert_eq!(days_to_ymd(19723), (2024, 1, 1));
    }

    #[test]
    fn test_days_to_ymd_leap_year() {
        // 2024-02-29 (leap day) is 19782 days since epoch
        assert_eq!(days_to_ymd(19782), (2024, 2, 29));
    }
}
