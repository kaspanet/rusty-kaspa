use crate::constants::{READ_BUFFER_SIZE, READ_TIMEOUT};
use crate::jsonrpc_event::JsonRpcEvent;
use crate::log_colors::LogColors;
use crate::stratum_context::StratumContext;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use hex;

/// Event handler function type
pub type EventHandler = Arc<dyn Fn(Arc<StratumContext>, JsonRpcEvent) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send>> + Send + Sync>;

/// Client listener trait
pub trait StratumClientListener: Send + Sync {
    fn on_connect(&self, ctx: Arc<StratumContext>);
    fn on_disconnect(&self, ctx: Arc<StratumContext>);
}

/// State generator function type
pub type StateGenerator = Box<dyn Fn() -> Arc<dyn std::any::Any + Send + Sync> + Send + Sync>;

/// Stratum listener statistics
#[derive(Debug, Default)]
pub struct StratumStats {
    pub disconnects: u64,
}

/// Configuration for the Stratum listener
pub struct StratumListenerConfig {
    pub handler_map: Arc<HashMap<String, EventHandler>>,
    pub on_connect: Arc<dyn Fn(Arc<StratumContext>) + Send + Sync>,
    pub on_disconnect: Arc<dyn Fn(Arc<StratumContext>) + Send + Sync>,
    pub port: String,
}

/// Stratum TCP listener
pub struct StratumListener {
    config: StratumListenerConfig,
    stats: Arc<parking_lot::Mutex<StratumStats>>,
    shutting_down: Arc<std::sync::atomic::AtomicBool>,
}

impl StratumListener {
    /// Create a new Stratum listener
    pub fn new(config: StratumListenerConfig) -> Self {
        Self {
            config,
            stats: Arc::new(parking_lot::Mutex::new(StratumStats::default())),
            shutting_down: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Start listening for connections
    pub async fn listen(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.shutting_down.store(false, std::sync::atomic::Ordering::Release);

        // Parse port - ensure we bind to IPv4 (0.0.0.0) to accept IPv4 connections
        // If it starts with ':', prepend "0.0.0.0", otherwise format as "0.0.0.0:PORT"
        let addr_str = if self.config.port.starts_with(':') {
            format!("0.0.0.0{}", self.config.port)
        } else {
            format!("0.0.0.0:{}", self.config.port)
        };
        
        let listener = TcpListener::bind(&addr_str).await
            .map_err(|e| format!("failed listening to socket {}: {}", self.config.port, e))?;

        tracing::debug!("Stratum listener started on {}", self.config.port);

        let (disconnect_tx, mut disconnect_rx) = mpsc::unbounded_channel::<Arc<StratumContext>>();
        let disconnect_tx_clone = disconnect_tx.clone();
        let on_disconnect = Arc::clone(&self.config.on_disconnect);
        let stats = self.stats.clone();

        // Spawn disconnect handler
        tokio::spawn(async move {
            while let Some(ctx) = disconnect_rx.recv().await {
                info!("[CONNECTION] client disconnecting - {}", ctx.remote_addr);
                tracing::info!("[CONNECTION] Disconnect event for {}:{}", ctx.remote_addr, ctx.remote_port);
                stats.lock().disconnects += 1;
                on_disconnect(ctx);
            }
        });

        // Accept connections
        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            let remote_addr = addr.ip().to_string();
                            let remote_port = addr.port();
                            
                            tracing::debug!("[CONNECTION] new client connecting - {}:{}", remote_addr, remote_port);
                            tracing::debug!("[CONNECTION] ===== TCP CONNECTION ESTABLISHED =====");
                            tracing::debug!("[CONNECTION] Remote address: {}:{}", remote_addr, remote_port);
                            tracing::debug!("[CONNECTION] Local address: {:?}", stream.local_addr());
                            tracing::debug!("[CONNECTION] Connection accepted successfully");
                            
                            // Create new MiningState for each client
                            // Each client gets its own isolated state, just like in Go
                            use crate::mining_state::MiningState;
                            let state = Arc::new(MiningState::new());
                            
                            // Clone for logging after move
                            let remote_addr_for_log = remote_addr.clone();
                            let remote_port_for_log = remote_port;
                            
                            tracing::debug!("[CONNECTION] Creating StratumContext for {}:{}", remote_addr_for_log, remote_port_for_log);
                            let ctx = StratumContext::new(
                                remote_addr,
                                remote_port,
                                stream,
                                state,
                                disconnect_tx_clone.clone(),
                            );
                            tracing::debug!("[CONNECTION] StratumContext created successfully");

                            tracing::debug!("[CONNECTION] Calling on_connect handler");
                            (self.config.on_connect)(ctx.clone());
                            tracing::debug!("[CONNECTION] on_connect handler completed");

                            // Spawn client handler
                            tracing::debug!("[CONNECTION] Spawning client listener task for {}:{}", remote_addr_for_log, remote_port_for_log);
                            let ctx_clone = ctx.clone();
                            let handler_map = self.config.handler_map.clone();
                            tokio::spawn(async move {
                                tracing::debug!("[CONNECTION] Client listener task started for {}:{}", ctx_clone.remote_addr, ctx_clone.remote_port);
                                Self::spawn_client_listener(ctx_clone, &handler_map).await;
                                tracing::debug!("[CONNECTION] Client listener task ended");
                            });
                            tracing::debug!("[CONNECTION] ===== CONNECTION SETUP COMPLETE FOR {}:{} =====", remote_addr_for_log, remote_port_for_log);
                        }
                        Err(e) => {
                            if self.shutting_down.load(std::sync::atomic::Ordering::Acquire) {
                                info!("stopping listening due to server shutdown");
                                break;
                            }
                            error!("[CONNECTION] ===== FAILED TO ACCEPT INCOMING CONNECTION =====");
                            error!("[CONNECTION] Error: {}", e);
                            error!("[CONNECTION] Error kind: {:?}", e.kind());
                            tracing::error!("[CONNECTION] Failed to accept connection: {} (kind: {:?})", e, e.kind());
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Spawn a client listener task
    async fn spawn_client_listener(
        ctx: Arc<StratumContext>,
        handler_map: &Arc<HashMap<String, EventHandler>>,
    ) {
        tracing::debug!("[CLIENT_LISTENER] Starting client listener for {}:{}", ctx.remote_addr, ctx.remote_port);
        let mut buffer = [0u8; READ_BUFFER_SIZE];
        let mut line_buffer = String::new();
        let mut first_message = true;

        loop {
            // Check if disconnected
            if !ctx.connected() {
                tracing::debug!("[CLIENT_LISTENER] Client {}:{} disconnected", ctx.remote_addr, ctx.remote_port);
                break;
            }

            // Get read half for reading (must drop guard before await)
            let read_half_opt = {
                let mut read_guard = ctx.get_read_half();
                read_guard.take()
            };
            
            let read_result = if let Some(mut read_half) = read_half_opt {
                // Set read deadline
                let deadline = tokio::time::Instant::now() + READ_TIMEOUT;
                
                let result = tokio::time::timeout_at(deadline, read_half.read(&mut buffer)).await;
                
                // Put read half back
                {
                    let mut read_guard = ctx.get_read_half();
                    *read_guard = Some(read_half);
                }
                
                result
            } else {
                // Read half is None, disconnect
                tracing::warn!("[CONNECTION] Read half is None for {}, disconnecting", ctx.remote_addr);
                break;
            };

            match read_result {
                Ok(Ok(0)) => {
                    // EOF - client closed connection
                    tracing::debug!("[CONNECTION] Client {} closed connection (EOF)", ctx.remote_addr);
                    break;
                }
                Ok(Ok(n)) => {
                    tracing::debug!("[CLIENT_LISTENER] Read {} bytes from {}:{}", n, ctx.remote_addr, ctx.remote_port);
                    
                    // Remove null bytes and process
                    let data: Vec<u8> = buffer[..n]
                        .iter()
                        .copied()
                        .filter(|&b| b != 0)
                        .collect();
                    
                    if first_message {
                        let wallet_addr = ctx.wallet_addr.lock().clone();
                        let worker_name = ctx.worker_name.lock().clone();
                        let remote_app = ctx.remote_app.lock().clone();
                        let message_str = String::from_utf8_lossy(&data);
                        
                        // Check for HTTP/2/gRPC protocol in first message (before logging)
                        let first_line = message_str.lines().next().unwrap_or("").trim();
                        if first_line.starts_with("PRI * HTTP/2.0") || 
                           first_line.starts_with("PRI * HTTP/2") ||
                           first_line == "SM" ||
                           first_line.starts_with("GET ") ||
                           first_line.starts_with("POST ") ||
                           first_line.starts_with("PUT ") ||
                           first_line.starts_with("DELETE ") ||
                           first_line.starts_with("HEAD ") ||
                           first_line.starts_with("OPTIONS ") {
                            error!("{}", LogColors::error("========================================"));
                            error!("{}", LogColors::error("===== PROTOCOL MISMATCH DETECTED (FIRST MESSAGE) ===== "));
                            error!("{}", LogColors::error("========================================"));
                            error!("{} {}", LogColors::error("[ERROR]"), LogColors::label("Client Information:"));
                            error!("{} {} {}", LogColors::error("[ERROR]"), LogColors::label("  - IP Address:"), format!("{}:{}", ctx.remote_addr, ctx.remote_port));
                            error!("{} {} {}", LogColors::error("[ERROR]"), LogColors::label("  - Protocol Detected:"), "HTTP/2 or HTTP (gRPC)");
                            error!("{} {} {}", LogColors::error("[ERROR]"), LogColors::label("  - Expected Protocol:"), "Plain TCP/JSON-RPC (Stratum)");
                            error!("{} {} {}", LogColors::error("[ERROR]"), LogColors::label("  - First Message (hex):"), hex::encode(&data));
                            error!("{} {} {}", LogColors::error("[ERROR]"), LogColors::label("  - First Message (string):"), first_line);
                            error!("{} {}", LogColors::error("[ERROR]"), LogColors::label("Action:"));
                            error!("{} {}", LogColors::error("[ERROR]"), "  * Rejecting connection - Stratum port only accepts JSON-RPC over plain TCP");
                            error!("{} {}", LogColors::error("[ERROR]"), "  * HTTP/2/gRPC connections should use the Kaspa node port (16110), not the bridge port (5555)");
                            error!("{} {}", LogColors::error("[ERROR]"), "  * Closing connection immediately");
                            error!("{}", LogColors::error("========================================"));
                            
                            // Close connection
                            ctx.disconnect();
                            break;
                        }
                        
                        tracing::debug!("{}", LogColors::asic_to_bridge("========================================"));
                        tracing::debug!("{}", LogColors::asic_to_bridge("===== FIRST MESSAGE FROM ASIC ===== "));
                        tracing::debug!("{}", LogColors::asic_to_bridge("========================================"));
                        tracing::debug!("{} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("Connection Information:"));
                        tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - IP Address:"), format!("{}:{}", ctx.remote_addr, ctx.remote_port));
                        tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Wallet Address:"), format!("'{}'", wallet_addr));
                        tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Worker Name:"), format!("'{}'", worker_name));
                        tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Miner Application:"), format!("'{}'", remote_app));
                        tracing::debug!("{} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("First Message Data:"));
                        tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Raw Bytes (hex):"), hex::encode(&data));
                        tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Raw Bytes Length:"), format!("{} bytes", data.len()));
                        tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Message as String:"), message_str);
                        tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - String Length:"), format!("{} characters", message_str.len()));
                        tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - String Length:"), format!("{} bytes (UTF-8)", message_str.len()));
                        // Show byte-by-byte breakdown for first 100 bytes
                        if data.len() <= 100 {
                            tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Byte Breakdown:"), format!("{:?}", data));
                        } else {
                            tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - First 100 Bytes:"), format!("{:?}", &data[..100.min(data.len())]));
                        }
                        tracing::debug!("{}", LogColors::asic_to_bridge("========================================"));
                        first_message = false;
                    }
                    
                    line_buffer.push_str(&String::from_utf8_lossy(&data));
                    
                    // Process complete lines
                    while let Some(newline_pos) = line_buffer.find('\n') {
                        let line = line_buffer[..newline_pos].trim().to_string();
                        line_buffer = line_buffer[newline_pos + 1..].to_string();
                        
                        if !line.is_empty() {
                            // Get client context for detailed logging
                            let wallet_addr = ctx.wallet_addr.lock().clone();
                            let worker_name = ctx.worker_name.lock().clone();
                            let remote_app = ctx.remote_app.lock().clone();
                            
                            // Detect HTTP/2/gRPC connections early and reject them
                            // HTTP/2 connection preface starts with "PRI * HTTP/2.0"
                            if line.starts_with("PRI * HTTP/2.0") || 
                               line.starts_with("PRI * HTTP/2") ||
                               line == "SM" ||
                               line.starts_with("GET ") ||
                               line.starts_with("POST ") ||
                               line.starts_with("PUT ") ||
                               line.starts_with("DELETE ") ||
                               line.starts_with("HEAD ") ||
                               line.starts_with("OPTIONS ") {
                                error!("{}", LogColors::error("========================================"));
                                error!("{}", LogColors::error("===== PROTOCOL MISMATCH DETECTED ===== "));
                                error!("{}", LogColors::error("========================================"));
                                error!("{} {}", LogColors::error("[ERROR]"), LogColors::label("Client Information:"));
                                error!("{} {} {}", LogColors::error("[ERROR]"), LogColors::label("  - IP Address:"), format!("{}:{}", ctx.remote_addr, ctx.remote_port));
                                error!("{} {} {}", LogColors::error("[ERROR]"), LogColors::label("  - Protocol Detected:"), "HTTP/2 or HTTP (gRPC)");
                                error!("{} {} {}", LogColors::error("[ERROR]"), LogColors::label("  - Expected Protocol:"), "Plain TCP/JSON-RPC (Stratum)");
                                error!("{} {} {}", LogColors::error("[ERROR]"), LogColors::label("  - Received Message:"), &line);
                                error!("{} {}", LogColors::error("[ERROR]"), LogColors::label("Action:"));
                                error!("{} {}", LogColors::error("[ERROR]"), "  * Rejecting connection - Stratum port only accepts JSON-RPC over plain TCP");
                                error!("{} {}", LogColors::error("[ERROR]"), "  * HTTP/2/gRPC connections should use the Kaspa node port (16110), not the bridge port (5555)");
                                error!("{} {}", LogColors::error("[ERROR]"), "  * Closing connection immediately");
                                error!("{}", LogColors::error("========================================"));
                                
                                // Close connection
                                ctx.disconnect();
                                break;
                            }
                            
                            // Log raw incoming message from ASIC at DEBUG level (verbose details)
                            tracing::debug!("{}", LogColors::asic_to_bridge("========================================"));
                            tracing::debug!("{}", LogColors::asic_to_bridge("===== RECEIVED MESSAGE FROM ASIC ===== "));
                            tracing::debug!("{}", LogColors::asic_to_bridge("========================================"));
                            tracing::debug!("{} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("Client Information:"));
                            tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - IP Address:"), format!("{}:{}", ctx.remote_addr, ctx.remote_port));
                            tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Wallet Address:"), format!("'{}'", wallet_addr));
                            tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Worker Name:"), format!("'{}'", worker_name));
                            tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Miner Application:"), format!("'{}'", remote_app));
                            tracing::debug!("{} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("Raw Message Data:"));
                            tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Raw Message:"), line);
                            tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Message Length:"), format!("{} bytes", line.len()));
                            tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Message Length:"), format!("{} characters", line.chars().count()));
                            tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Raw Bytes (hex):"), hex::encode(line.as_bytes()));
                            
                            match crate::jsonrpc_event::unmarshal_event(&line) {
                                Ok(event) => {
                                    let params_str = serde_json::to_string(&event.params).unwrap_or_else(|_| "[]".to_string());
                                    
                                    // Log parsed event details at DEBUG level (detailed logs moved to debug)
                                    tracing::debug!("{}", LogColors::asic_to_bridge("===== PARSING SUCCESSFUL ===== "));
                                    tracing::debug!("{} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("Parsed Event Structure:"));
                                    tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Method:"), format!("'{}'", event.method));
                                    tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Event ID:"), format!("{:?}", event.id));
                                    tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - JSON-RPC Version:"), format!("'{}'", event.jsonrpc));
                                    tracing::debug!("{} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("Parameters:"));
                                    tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Params Count:"), event.params.len());
                                    tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Params JSON:"), params_str);
                                    tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Params Length:"), format!("{} characters", params_str.len()));
                                    // Log each param individually with type information
                                    for (idx, param) in event.params.iter().enumerate() {
                                        let param_str = serde_json::to_string(param).unwrap_or_else(|_| "N/A".to_string());
                                        let param_type = if param.is_string() { 
                                            let s = param.as_str().unwrap_or("");
                                            format!("String (length: {}, value: '{}')", s.len(), s)
                                        } 
                                        else if param.is_number() { 
                                            format!("Number (value: {})", param)
                                        } 
                                        else if param.is_array() { 
                                            let arr = param.as_array().unwrap();
                                            format!("Array (length: {}, items: {:?})", arr.len(), 
                                                    arr.iter().take(5).map(|v| serde_json::to_string(v).unwrap_or_else(|_| "?".to_string())).collect::<Vec<_>>())
                                        } 
                                        else if param.is_object() { 
                                            "Object".to_string()
                                        } 
                                        else if param.is_boolean() { 
                                            format!("Boolean (value: {})", param.as_bool().unwrap_or(false))
                                        } 
                                        else { 
                                            "Null".to_string()
                                        };
                                        tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label(&format!("  - Param[{}]:", idx)), format!("{} (type: {})", param_str, param_type));
                                    }
                                    
                                    if let Some(handler) = handler_map.get(&event.method) {
                                        tracing::debug!("{}", LogColors::asic_to_bridge("===== PROCESSING MESSAGE ===== "));
                                        tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Handler Found:"), "YES");
                                        tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Method:"), format!("'{}'", event.method));
                                        tracing::debug!("{} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), "  - Starting handler execution...");
                                        if let Err(e) = handler(ctx.clone(), event).await {
                                            let error_msg = e.to_string();
                                            if error_msg.contains("stale") || error_msg.contains("job does not exist") {
                                                // Log stale job errors as debug (expected behavior, not important)
                                                tracing::debug!("{}", LogColors::asic_to_bridge("===== HANDLER EXECUTION RESULT ===== "));
                                                tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::validation("  - Result:"), "STALE JOB (expected - job no longer exists)");
                                                tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Error Message:"), error_msg);
                                            } else if error_msg.contains("job id is not parsable") {
                                                // Log parsing errors as warnings
                                                warn!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::error("  - Result:"), "ERROR (job ID parsing failed)");
                                                warn!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Error Message:"), error_msg);
                                            } else {
                                                error!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::error("  - Result:"), "ERROR (handler execution failed)");
                                                error!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Error Message:"), error_msg);
                                            }
                                        } else {
                                            tracing::debug!("{}", LogColors::asic_to_bridge("===== HANDLER EXECUTION RESULT ===== "));
                                            tracing::debug!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Result:"), "SUCCESS");
                                            tracing::debug!("{} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), "  - Message processed successfully");
                                        }
                                        tracing::debug!("{}", LogColors::asic_to_bridge("========================================"));
                                    }
                                }
                                Err(e) => {
                                    error!("{}", LogColors::asic_to_bridge("========================================"));
                                    error!("{}", LogColors::error("===== ERROR PARSING MESSAGE ===== "));
                                    error!("{}", LogColors::asic_to_bridge("========================================"));
                                    error!("{} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("Client Information:"));
                                    error!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - IP Address:"), format!("{}:{}", ctx.remote_addr, ctx.remote_port));
                                    error!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Wallet Address:"), format!("'{}'", wallet_addr));
                                    error!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Worker Name:"), format!("'{}'", worker_name));
                                    error!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Miner Application:"), format!("'{}'", remote_app));
                                    error!("{} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("Failed Message:"));
                                    error!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Raw Message:"), line);
                                    error!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Message Length:"), format!("{} bytes", line.len()));
                                    error!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Raw Bytes (hex):"), hex::encode(line.as_bytes()));
                                    error!("{} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("Parse Error Details:"));
                                    error!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Error Type:"), "JSON Parsing Failed");
                                    error!("{} {} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::error("  - Error Message:"), e);
                                    error!("{} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), LogColors::label("  - Possible Causes:"));
                                    error!("{} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), "    * Malformed JSON syntax");
                                    error!("{} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), "    * Protocol mismatch");
                                    error!("{} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), "    * Incomplete message");
                                    error!("{} {}", LogColors::asic_to_bridge("[ASIC->BRIDGE]"), "    * Encoding issue");
                                    error!("{}", LogColors::asic_to_bridge("========================================"));
                                }
                            }
                        }
                    }
                }
                Ok(Err(e)) => {
                    // Check if it's a connection closed error (expected when client disconnects)
                    let error_msg = e.to_string();
                    if error_msg.contains("forcibly closed") || 
                       error_msg.contains("Connection reset") ||
                       error_msg.contains("Broken pipe") ||
                       e.kind() == std::io::ErrorKind::ConnectionReset ||
                       e.kind() == std::io::ErrorKind::BrokenPipe {
                        tracing::debug!("client disconnected: {}", ctx.remote_addr);
                    } else {
                        error!("error reading from socket: {}", e);
                    }
                    break;
                }
                Err(_) => {
                    // Timeout - continue
                    tokio::time::sleep(crate::constants::SOCKET_WAIT_DELAY).await;
                    continue;
                }
            }
        }

        ctx.disconnect();
    }

    /// Handle an event
    pub fn handle_event(
        &self,
        _ctx: Arc<StratumContext>,
        event: JsonRpcEvent,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(_handler) = self.config.handler_map.get(&event.method) {
            // Note: This is a sync wrapper - actual handlers should be async
            // For now, we'll handle this in spawn_client_listener
            Ok(())
        } else {
            Ok(())
        }
    }
}

