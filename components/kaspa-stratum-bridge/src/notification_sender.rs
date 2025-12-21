//! Notification Sending Helper
//!
//! This module provides helper functions for sending notifications to miners
//! with the correct format based on miner type. All logic preserves exact
//! existing behavior from the codebase.

use crate::jsonrpc_event::JsonRpcEvent;
use crate::miner_detection::is_iceriver;
use crate::stratum_context::{ErrorDisconnected, StratumContext};
use serde_json::Value;

/// Send a mining.notify notification with appropriate format
///
/// This function preserves the EXACT notification format logic from the codebase:
/// - IceRiver: Minimal format (method + params only, no id or jsonrpc)
/// - Others: Standard JSON-RPC format (jsonrpc: "2.0", method, id, params)
///
/// # Arguments
/// * `client` - The stratum context for the client
/// * `method` - The method name (typically "mining.notify")
/// * `params` - The parameters for the notification
/// * `job_id` - The job ID (used for standard format, ignored for IceRiver)
/// * `remote_app` - The remote app string (for miner type detection)
///
/// # Returns
/// Result indicating success or failure of sending the notification
pub async fn send_mining_notification(
    client: &StratumContext,
    method: &str,
    params: Vec<Value>,
    job_id: u64,
    remote_app: &str,
) -> Result<(), ErrorDisconnected> {
    let is_iceriver_flag = is_iceriver(remote_app);

    if is_iceriver_flag {
        // IceRiver expects minimal notification format (method + params only, no id or jsonrpc)
        client.send_notification(method, params).await
    } else {
        // For non-IceRiver, use standard JSON-RPC format with job ID
        let notify_event =
            JsonRpcEvent { jsonrpc: "2.0".to_string(), method: method.to_string(), id: Some(Value::Number(job_id.into())), params };
        client.send(notify_event).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_format_selection() {
        // Test that IceRiver detection works
        assert!(is_iceriver("IceRiver KS2L"));
        assert!(!is_iceriver("GodMiner"));
        assert!(!is_iceriver("BzMiner"));
    }
}
