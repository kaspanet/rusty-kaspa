use crate::constants::{WRITE_TIMEOUT, WRITE_MAX_RETRIES, WRITE_RETRY_DELAY};
use crate::jsonrpc_event::{JsonRpcEvent, JsonRpcResponse};
use crate::log_colors::LogColors;
use hex;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::mpsc;

/// Error for disconnected clients
#[derive(Debug, thiserror::Error)]
#[error("disconnecting")]
pub struct ErrorDisconnected;

/// Context summary for logging
#[derive(Debug, Clone)]
pub struct ContextSummary {
    pub remote_addr: String,
    pub remote_port: u16,
    pub wallet_addr: String,
    pub worker_name: String,
    pub remote_app: String,
}

/// Stratum client context
pub struct StratumContext {
    pub remote_addr: String,
    pub remote_port: u16,
    pub wallet_addr: Arc<Mutex<String>>,
    pub worker_name: Arc<Mutex<String>>,
    pub canxium_addr: Arc<Mutex<String>>,
    pub remote_app: Arc<Mutex<String>>,
    pub id: Arc<Mutex<i32>>,
    pub extranonce: Arc<Mutex<String>>,
    pub state: Arc<crate::mining_state::MiningState>,
    disconnecting: Arc<AtomicBool>,
    write_lock: Arc<AtomicBool>,
    read_half: Arc<Mutex<Option<tokio::io::ReadHalf<TcpStream>>>>,
    write_half: Arc<Mutex<Option<tokio::io::WriteHalf<TcpStream>>>>,
    on_disconnect: mpsc::UnboundedSender<Arc<StratumContext>>,
}

impl StratumContext {
    pub fn new(
        remote_addr: String,
        remote_port: u16,
        stream: TcpStream,
        state: Arc<crate::mining_state::MiningState>,
        on_disconnect: mpsc::UnboundedSender<Arc<StratumContext>>,
    ) -> Arc<Self> {
        let (read_half, write_half) = tokio::io::split(stream);
        Arc::new(Self {
            remote_addr,
            remote_port,
            wallet_addr: Arc::new(Mutex::new(String::new())),
            worker_name: Arc::new(Mutex::new(String::new())),
            canxium_addr: Arc::new(Mutex::new(String::new())),
            remote_app: Arc::new(Mutex::new(String::new())),
            id: Arc::new(Mutex::new(0)),
            extranonce: Arc::new(Mutex::new(String::new())),
            state,
            disconnecting: Arc::new(AtomicBool::new(false)),
            write_lock: Arc::new(AtomicBool::new(false)),
            read_half: Arc::new(Mutex::new(Some(read_half))),
            write_half: Arc::new(Mutex::new(Some(write_half))),
            on_disconnect,
        })
    }

    /// Check if client is connected
    pub fn connected(&self) -> bool {
        !self.disconnecting.load(Ordering::Acquire)
    }

    /// Get client ID
    pub fn id(&self) -> Option<i32> {
        let id = *self.id.lock();
        if id > 0 {
            Some(id)
        } else {
            None
        }
    }

    /// Set client ID
    pub fn set_id(&self, id: i32) {
        *self.id.lock() = id;
    }

    /// Get context summary
    pub fn summary(&self) -> ContextSummary {
        ContextSummary {
            remote_addr: self.remote_addr.clone(),
            remote_port: self.remote_port,
            wallet_addr: self.wallet_addr.lock().clone(),
            worker_name: self.worker_name.lock().clone(),
            remote_app: self.remote_app.lock().clone(),
        }
    }

    /// Get remote address string
    pub fn remote_addr(&self) -> &str {
        &self.remote_addr
    }

    /// Get remote port
    pub fn remote_port(&self) -> u16 {
        self.remote_port
    }

    /// Send a JSON-RPC response
    pub async fn reply(&self, response: JsonRpcResponse) -> Result<(), ErrorDisconnected> {
        if self.disconnecting.load(Ordering::Acquire) {
            return Err(ErrorDisconnected);
        }

        let json = serde_json::to_string(&response)
            .map_err(|_| ErrorDisconnected)?;
        let data = format!("{}\n", json);

        // Get client context for detailed logging
        let wallet_addr = self.wallet_addr.lock().clone();
        let worker_name = self.worker_name.lock().clone();
        let remote_app = self.remote_app.lock().clone();
        
        // Log outgoing response at DEBUG level (detailed logs moved to debug)
        tracing::debug!("{}", LogColors::bridge_to_asic("========================================"));
        tracing::debug!("{}", LogColors::bridge_to_asic("===== SENDING RESPONSE TO ASIC ===== "));
        tracing::debug!("{}", LogColors::bridge_to_asic("========================================"));
        tracing::debug!("{} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("Client Information:"));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - IP Address:"), format!("{}:{}", self.remote_addr, self.remote_port));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Wallet Address:"), format!("'{}'", wallet_addr));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Worker Name:"), format!("'{}'", worker_name));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Miner Application:"), format!("'{}'", remote_app));
        tracing::debug!("{} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("Response Details:"));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Response ID:"), format!("{:?}", response.id));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Response Type:"), "JSON-RPC Response");
        if let Some(ref result) = response.result {
            let result_str = serde_json::to_string(result).unwrap_or_else(|_| "N/A".to_string());
            tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Result:"), result_str);
            tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Result Length:"), format!("{} characters", result_str.len()));
        }
        if let Some(ref error) = response.error {
            let error_str = serde_json::to_string(error).unwrap_or_else(|_| "N/A".to_string());
            tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::error("  - Error:"), error_str);
            tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Error Length:"), format!("{} characters", error_str.len()));
        }
        tracing::debug!("{} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("Message Data:"));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Raw JSON:"), json);
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - JSON Length:"), format!("{} characters", json.len()));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Total Bytes (with newline):"), format!("{} bytes", data.len()));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Raw Bytes (hex):"), hex::encode(data.as_bytes()));
        tracing::debug!("{}", LogColors::bridge_to_asic("========================================"));

        self.write_data(data.as_bytes()).await?;
        Ok(())
    }

    /// Send a JSON-RPC event
    pub async fn send(&self, event: JsonRpcEvent) -> Result<(), ErrorDisconnected> {
        if self.disconnecting.load(Ordering::Acquire) {
            return Err(ErrorDisconnected);
        }

        let json = serde_json::to_string(&event)
            .map_err(|_| ErrorDisconnected)?;
        let data = format!("{}\n", json);

        // Get client context for detailed logging
        let wallet_addr = self.wallet_addr.lock().clone();
        let worker_name = self.worker_name.lock().clone();
        let remote_app = self.remote_app.lock().clone();
        let params_str = serde_json::to_string(&event.params).unwrap_or_else(|_| "[]".to_string());
        
        // Log outgoing event at DEBUG level (detailed logs moved to debug)
        tracing::debug!("{}", LogColors::bridge_to_asic("========================================"));
        tracing::debug!("{}", LogColors::bridge_to_asic("===== SENDING EVENT TO ASIC ===== "));
        tracing::debug!("{}", LogColors::bridge_to_asic("========================================"));
        tracing::debug!("{} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("Client Information:"));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - IP Address:"), format!("{}:{}", self.remote_addr, self.remote_port));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Wallet Address:"), format!("'{}'", wallet_addr));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Worker Name:"), format!("'{}'", worker_name));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Miner Application:"), format!("'{}'", remote_app));
        tracing::debug!("{} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("Event Details:"));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Method:"), format!("'{}'", event.method));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Event ID:"), format!("{:?}", event.id));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - JSON-RPC Version:"), format!("'{}'", event.jsonrpc));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Format:"), "Standard JSON-RPC (with jsonrpc field)");
        tracing::debug!("{} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("Parameters:"));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Params Count:"), event.params.len());
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Params JSON:"), params_str);
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Params Length:"), format!("{} characters", params_str.len()));
        // Log each param individually
        for (idx, param) in event.params.iter().enumerate() {
            let param_str = serde_json::to_string(param).unwrap_or_else(|_| "N/A".to_string());
            let param_type = if param.is_string() { 
                "String".to_string()
            } 
            else if param.is_number() { 
                "Number".to_string()
            } 
            else if param.is_array() { 
                "Array".to_string()
            } 
            else if param.is_object() { 
                "Object".to_string()
            } 
            else if param.is_boolean() { 
                "Boolean".to_string()
            } 
            else { 
                "Null".to_string()
            };
            tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label(&format!("  - Param[{}]:", idx)), format!("{} (type: {})", param_str, param_type));
        }
        tracing::debug!("{} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("Message Data:"));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Raw JSON:"), json);
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - JSON Length:"), format!("{} characters", json.len()));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Total Bytes (with newline):"), format!("{} bytes", data.len()));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Raw Bytes (hex):"), hex::encode(data.as_bytes()));
        tracing::debug!("{}", LogColors::bridge_to_asic("========================================"));

        self.write_data(data.as_bytes()).await?;
        Ok(())
    }

    /// Send a minimal Stratum notification (method + params only, no id or jsonrpc)
    /// This matches the format used by the stratum crate and expected by IceRiver ASICs
    pub async fn send_notification(&self, method: &str, params: Vec<serde_json::Value>) -> Result<(), ErrorDisconnected> {
        if self.disconnecting.load(Ordering::Acquire) {
            return Err(ErrorDisconnected);
        }

        // Manually construct JSON without id or jsonrpc fields (matches StratumNotification format)
        let notification = serde_json::json!({
            "method": method,
            "params": params
        });
        
        let json = serde_json::to_string(&notification)
            .map_err(|_| ErrorDisconnected)?;
        let data = format!("{}\n", json);

        // Get client context for detailed logging
        let wallet_addr = self.wallet_addr.lock().clone();
        let worker_name = self.worker_name.lock().clone();
        let remote_app = self.remote_app.lock().clone();
        let params_str = serde_json::to_string(&params).unwrap_or_else(|_| "[]".to_string());
        
        // Log outgoing notification at DEBUG level (detailed logs moved to debug)
        tracing::debug!("{}", LogColors::bridge_to_asic("========================================"));
        tracing::debug!("{}", LogColors::bridge_to_asic("===== SENDING NOTIFICATION TO ASIC ===== "));
        tracing::debug!("{}", LogColors::bridge_to_asic("========================================"));
        tracing::debug!("{} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("Client Information:"));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - IP Address:"), format!("{}:{}", self.remote_addr, self.remote_port));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Wallet Address:"), format!("'{}'", wallet_addr));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Worker Name:"), format!("'{}'", worker_name));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Miner Application:"), format!("'{}'", remote_app));
        tracing::debug!("{} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("Notification Details:"));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Method:"), format!("'{}'", method));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Format:"), "Minimal Stratum (no id/jsonrpc fields)");
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Target:"), "IceRiver/BzMiner compatible");
        tracing::debug!("{} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("Parameters:"));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Params Count:"), params.len());
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Params JSON:"), params_str);
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Params Length:"), format!("{} characters", params_str.len()));
        // Log each param individually
        for (idx, param) in params.iter().enumerate() {
            let param_str = serde_json::to_string(param).unwrap_or_else(|_| "N/A".to_string());
            let param_type = if param.is_string() { 
                format!("String (length: {})", param.as_str().map(|s| s.len()).unwrap_or(0))
            } 
            else if param.is_number() { 
                "Number".to_string()
            } 
            else if param.is_array() { 
                format!("Array (length: {})", param.as_array().map(|a| a.len()).unwrap_or(0))
            } 
            else if param.is_object() { 
                "Object".to_string()
            } 
            else if param.is_boolean() { 
                "Boolean".to_string()
            } 
            else { 
                "Null".to_string()
            };
            tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label(&format!("  - Param[{}]:", idx)), format!("{} (type: {})", param_str, param_type));
        }
        tracing::debug!("{} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("Message Data:"));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Raw JSON:"), json);
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - JSON Length:"), format!("{} characters", json.len()));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Total Bytes (with newline):"), format!("{} bytes", data.len()));
        tracing::debug!("{} {} {}", LogColors::bridge_to_asic("[BRIDGE->ASIC]"), LogColors::label("  - Raw Bytes (hex):"), hex::encode(data.as_bytes()));
        tracing::debug!("{}", LogColors::bridge_to_asic("========================================"));

        self.write_data(data.as_bytes()).await?;
        Ok(())
    }

    /// Write data to the connection with backoff
    async fn write_data(&self, data: &[u8]) -> Result<(), ErrorDisconnected> {
        // Check if already disconnected
        if self.disconnecting.load(Ordering::Acquire) {
            return Err(ErrorDisconnected);
        }

        for attempt in 0..3 {
            if self.write_lock.compare_exchange(
                false,
                true,
                Ordering::Acquire,
                Ordering::Relaxed,
            ).is_ok() {
                // Extract write half (drop guard before await)
                let write_half_opt = {
                    let mut write_guard = self.write_half.lock();
                    write_guard.take()
                };
                
                let result = if let Some(mut write_half) = write_half_opt {
                    let deadline = tokio::time::Instant::now() + WRITE_TIMEOUT;
                    
                    // Try to write directly (no need to wait for writable)
                    let write_result = tokio::time::timeout_at(deadline, write_half.write_all(data)).await;
                            
                            // Put write half back regardless of result
                            {
                                let mut write_guard = self.write_half.lock();
                                *write_guard = Some(write_half);
                            }
                            
                            write_result
                } else {
                    self.write_lock.store(false, Ordering::Release);
                    return Err(ErrorDisconnected);
                };

                self.write_lock.store(false, Ordering::Release);

                match result {
                    Ok(Ok(_)) => return Ok(()),
                    Ok(Err(e)) => {
                        tracing::warn!("Write error: {}", e);
                        self.check_disconnect();
                        return Err(ErrorDisconnected);
                    }
                    Err(_) => {
                        // Timeout on write - try again if we have attempts left
                        if attempt < WRITE_MAX_RETRIES - 1 {
                            tokio::time::sleep(WRITE_RETRY_DELAY).await;
                            continue;
                        } else {
                            self.check_disconnect();
                            return Err(ErrorDisconnected);
                        }
                    }
                }
            } else {
                // Write blocked - wait and retry
                tokio::time::sleep(WRITE_RETRY_DELAY).await;
            }
        }

        Err(ErrorDisconnected)
    }

    /// Reply with stale share error
    pub async fn reply_stale_share(&self, id: Option<Value>) -> Result<(), ErrorDisconnected> {
        tracing::debug!("[BRIDGE->ASIC] Preparing STALE SHARE response (Error Code: 21, Job not found)");
        self.reply(JsonRpcResponse::error(id, 21, "Job not found", None)).await
    }

    /// Reply with duplicate share error
    pub async fn reply_dupe_share(&self, id: Option<Value>) -> Result<(), ErrorDisconnected> {
        tracing::debug!("[BRIDGE->ASIC] Preparing DUPLICATE SHARE response (Error Code: 22, Duplicate share submitted)");
        self.reply(JsonRpcResponse::error(id, 22, "Duplicate share submitted", None)).await
    }

    /// Reply with bad share error
    pub async fn reply_bad_share(&self, id: Option<Value>) -> Result<(), ErrorDisconnected> {
        tracing::debug!("[BRIDGE->ASIC] Preparing BAD SHARE response (Error Code: 20, Unknown problem)");
        self.reply(JsonRpcResponse::error(id, 20, "Unknown problem", None)).await
    }

    /// Reply with low difficulty share error
    pub async fn reply_low_diff_share(&self, id: &serde_json::Value) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tracing::debug!("[BRIDGE->ASIC] Preparing LOW DIFFICULTY SHARE response (Error Code: 23, Invalid difficulty)");
        self.reply(JsonRpcResponse::error(Some(id.clone()), 23, "Invalid difficulty", None)).await
            .map_err(|e| Box::new(std::io::Error::other(e.to_string())) as Box<dyn std::error::Error + Send + Sync>)
    }


    /// Send a response (async)
    #[allow(dead_code)]
    async fn send_response(&self, response: JsonRpcResponse) -> Result<(), ErrorDisconnected> {
        let json = serde_json::to_string(&response)
            .map_err(|_| ErrorDisconnected)?;
        let data = format!("{}\n", json);
        self.write_data(data.as_bytes()).await
    }

    /// Disconnect the client
    pub fn disconnect(&self) {
        if !self.disconnecting.swap(true, Ordering::Release) {
            tracing::info!("disconnecting client {}", self.remote_addr);
            
            // Close the write half
            let write_half_opt = {
                let mut write_guard = self.write_half.lock();
                write_guard.take()
            };
            
            if let Some(mut write_half) = write_half_opt {
                // Try to shutdown gracefully in async context
                tokio::spawn(async move {
                    let _ = write_half.shutdown().await;
                });
            }
            
            // Close the read half
            let _ = {
                let mut read_guard = self.read_half.lock();
                read_guard.take()
            };
        }
    }

    fn check_disconnect(&self) {
        if !self.disconnecting.load(Ordering::Acquire) {
            // Spawn async disconnect
            let ctx = self.clone();
            tokio::spawn(async move {
                ctx.disconnect();
            });
        }
    }

    /// Get a reference to the read half (for reading)
    pub fn get_read_half(&self) -> parking_lot::MutexGuard<'_, Option<tokio::io::ReadHalf<TcpStream>>> {
        self.read_half.lock()
    }
}

impl Clone for StratumContext {
    fn clone(&self) -> Self {
        Self {
            remote_addr: self.remote_addr.clone(),
            remote_port: self.remote_port,
            wallet_addr: self.wallet_addr.clone(),
            worker_name: self.worker_name.clone(),
            canxium_addr: self.canxium_addr.clone(),
            remote_app: self.remote_app.clone(),
            id: self.id.clone(),
            extranonce: self.extranonce.clone(),
            state: self.state.clone(),
            disconnecting: self.disconnecting.clone(),
            write_lock: self.write_lock.clone(),
            read_half: self.read_half.clone(),
            write_half: self.write_half.clone(),
            on_disconnect: self.on_disconnect.clone(),
        }
    }
}

use serde_json::Value;

