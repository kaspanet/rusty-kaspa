use crate::jsonrpc_event::{JsonRpcEvent, JsonRpcResponse};
use crate::stratum_context::StratumContext;
use kaspa_addresses::Address;
use regex::Regex;
use serde_json::Value;
use std::sync::{Arc, LazyLock};

/// Regex for matching miners that use big job format
/// Matches: BzMiner, IceRiverMiner (from client_handler.go bigJobRegex)
static BIG_JOB_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r".*(BzMiner|IceRiverMiner).*").unwrap());

/// Regex for matching wallet addresses
static WALLET_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"kaspa(test|dev)?:([a-z0-9]{61}|[a-z0-9]{63})").unwrap());

/// Default logger configuration
pub fn default_logger() {
    // Logger is configured via tracing-subscriber in main
    // This function is kept for API compatibility
}

/// Default handler map
pub fn default_handlers() -> std::collections::HashMap<String, crate::stratum_listener::EventHandler> {
    let mut handlers = std::collections::HashMap::new();

    handlers.insert(
        "mining.subscribe".to_string(),
        Arc::new(|ctx: Arc<StratumContext>, event: JsonRpcEvent| {
            let ctx = ctx.clone();
            let event = event.clone();
            Box::pin(async move { handle_subscribe(ctx, event, None).await })
                as std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send>>
        }) as crate::stratum_listener::EventHandler,
    );

    handlers.insert(
        "mining.extranonce.subscribe".to_string(),
        Arc::new(|ctx: Arc<StratumContext>, event: JsonRpcEvent| {
            let ctx = ctx.clone();
            let event = event.clone();
            Box::pin(async move { handle_extranonce_subscribe(ctx, event).await })
                as std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send>>
        }) as crate::stratum_listener::EventHandler,
    );

    handlers.insert(
        "mining.authorize".to_string(),
        Arc::new(|ctx: Arc<StratumContext>, event: JsonRpcEvent| {
            let ctx = ctx.clone();
            let event = event.clone();
            Box::pin(async move {
                // Default handler - no client_handler/kaspa_api (will use polling fallback)
                handle_authorize(ctx, event, None, None).await
            })
                as std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send>>
        }) as crate::stratum_listener::EventHandler,
    );

    handlers.insert(
        "mining.submit".to_string(),
        Arc::new(|ctx: Arc<StratumContext>, event: JsonRpcEvent| {
            let ctx = ctx.clone();
            let event = event.clone();
            Box::pin(async move { handle_submit(ctx, event).await })
                as std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send>>
        }) as crate::stratum_listener::EventHandler,
    );

    handlers
}

/// Handle subscribe request
pub async fn handle_subscribe(
    ctx: Arc<StratumContext>,
    event: JsonRpcEvent,
    client_handler: Option<Arc<crate::client_handler::ClientHandler>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing::debug!("[SUBSCRIBE] ===== SUBSCRIBE REQUEST FROM {} =====", ctx.remote_addr);
    tracing::debug!("[SUBSCRIBE] Event ID: {:?}", event.id);
    tracing::debug!("[SUBSCRIBE] Params count: {}", event.params.len());

    // Extract remote app from params if present
    if let Some(Value::String(app)) = event.params.first() {
        *ctx.remote_app.lock() = app.clone();
        tracing::debug!("[SUBSCRIBE] Extracted app from params[0]: '{}'", app);
    } else {
        tracing::warn!("[SUBSCRIBE] No app string in params[0], params: {:?}", event.params);
    }

    let remote_app = ctx.remote_app.lock().clone();

    // Auto-detect miner type and assign appropriate extranonce
    if let Some(handler) = client_handler {
        handler.assign_extranonce_for_miner(&ctx, &remote_app);
    }

    let extranonce = ctx.extranonce.lock().clone();

    tracing::debug!("[SUBSCRIBE] Client info - app: '{}', extranonce: '{}', addr: {}", remote_app, extranonce, ctx.remote_addr);

    // Check if this is a Bitmain miner - use same detection logic as assign_extranonce_for_miner
    // (case-insensitive matching for consistency)
    let is_bitmain_flag = crate::miner_detection::is_bitmain(&remote_app);
    tracing::debug!("[SUBSCRIBE] Detected miner type - Remote app: '{}', Is Bitmain: {}", remote_app, is_bitmain_flag);

    if is_bitmain_flag {
        tracing::debug!("[SUBSCRIBE] ===== BITMAIN MINER DETECTED =====");
        tracing::debug!("[SUBSCRIBE] Bitmain requires extranonce in subscribe response");
        tracing::debug!("[SUBSCRIBE] Current extranonce: '{}' (length: {} bytes)", extranonce, extranonce.len() / 2);
    }

    let response = if is_bitmain_flag {
        // Bitmain format - extranonce in subscribe response
        let extranonce2_size = 8 - (extranonce.len() / 2);
        tracing::debug!("[SUBSCRIBE] ===== USING BITMAIN SUBSCRIBE FORMAT FOR {} =====", ctx.remote_addr);
        tracing::debug!("[SUBSCRIBE] Bitmain extranonce: '{}', extranonce2_size: {}", extranonce, extranonce2_size);
        tracing::debug!("[SUBSCRIBE] Bitmain response: [null, '{}', {}]", extranonce, extranonce2_size);
        JsonRpcResponse::new(
            &event,
            Some(Value::Array(vec![Value::Null, Value::String(extranonce.clone()), Value::Number(extranonce2_size.into())])),
            None,
        )
    } else {
        // Standard format (for IceRiver, BzMiner, and other miners)
        // Extranonce will be sent via mining.set_extranonce after authorize
        if BIG_JOB_REGEX.is_match(&remote_app) {
            tracing::debug!("[SUBSCRIBE] Using standard subscribe format for IceRiver/BzMiner {}", ctx.remote_addr);
        } else {
            tracing::debug!("[SUBSCRIBE] Using standard subscribe format for {}", ctx.remote_addr);
        }
        tracing::debug!("[SUBSCRIBE] Standard response: [true, 'EthereumStratum/1.0.0']");
        JsonRpcResponse::new(
            &event,
            Some(Value::Array(vec![Value::Bool(true), Value::String("EthereumStratum/1.0.0".to_string())])),
            None,
        )
    };

    let response_json = serde_json::to_string(&response).unwrap_or_else(|_| "failed".to_string());
    tracing::debug!("[SUBSCRIBE] Sending subscribe response to {}: {}", ctx.remote_addr, response_json);

    ctx.reply(response).await.map_err(|e| format!("failed to send response to subscribe: {}", e))?;

    tracing::debug!("[SUBSCRIBE] ===== SUBSCRIBE COMPLETE FOR {} =====", ctx.remote_addr);
    Ok(())
}

/// Handle extranonce subscribe request
async fn handle_extranonce_subscribe(
    ctx: Arc<StratumContext>,
    event: JsonRpcEvent,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing::debug!("[EXTRANONCE_SUBSCRIBE] ===== EXTRANONCE SUBSCRIBE FROM {} =====", ctx.remote_addr);
    tracing::debug!("[EXTRANONCE_SUBSCRIBE] Event ID: {:?}", event.id);

    let response = JsonRpcResponse::new(&event, Some(Value::Bool(true)), None);
    let response_json = serde_json::to_string(&response).unwrap_or_else(|_| "failed".to_string());
    tracing::debug!("[EXTRANONCE_SUBSCRIBE] Sending response to {}: {}", ctx.remote_addr, response_json);

    ctx.reply(response).await.map_err(|e| format!("failed to send response to extranonce subscribe: {}", e))?;

    tracing::debug!("[EXTRANONCE_SUBSCRIBE] ===== EXTRANONCE SUBSCRIBE COMPLETE FOR {} =====", ctx.remote_addr);
    Ok(())
}

/// Handle authorize request (v0.1 canxium-patch)
/// If client_handler and kaspa_api are provided, sends immediate job after authorization
pub async fn handle_authorize(
    ctx: Arc<StratumContext>,
    event: JsonRpcEvent,
    client_handler: Option<Arc<crate::client_handler::ClientHandler>>,
    kaspa_api: Option<Arc<dyn crate::share_handler::KaspaApiTrait + Send + Sync>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing::debug!("[AUTHORIZE] ===== AUTHORIZE REQUEST FROM {} =====", ctx.remote_addr);
    tracing::debug!("[AUTHORIZE] Event ID: {:?}", event.id);
    tracing::debug!("[AUTHORIZE] Params count: {}", event.params.len());
    tracing::debug!("[AUTHORIZE] Full params: {:?}", event.params);

    if event.params.is_empty() {
        tracing::error!("[AUTHORIZE] ERROR: Empty params from {}", ctx.remote_addr);
        return Err("malformed event from miner, expected param[0] to be address".into());
    }

    let address_value = event.params.first().ok_or("missing address parameter")?;

    let address_str = address_value.as_str().ok_or("expected param[0] to be address string")?;

    tracing::debug!("[AUTHORIZE] Address string from params[0]: '{}'", address_str);

    let parts: Vec<&str> = address_str.split('.').collect();
    tracing::debug!("[AUTHORIZE] Split address into {} parts: {:?}", parts.len(), parts);

    let mut address = parts[0].to_string();
    let mut worker_name = String::new();
    let mut canxium_address = String::new();

    if parts.len() >= 2 {
        worker_name = parts[1].to_string();
        tracing::debug!("[AUTHORIZE] Extracted worker name: '{}'", worker_name);
        if parts.len() >= 3 {
            canxium_address = process_canxium_address(parts[2]);
            tracing::debug!("[AUTHORIZE] Extracted canxium address: '{}'", canxium_address);
        }
    }

    // Clean and validate wallet address
    tracing::debug!("[AUTHORIZE] Cleaning wallet address: '{}'", address);
    address = clean_wallet(&address)?;
    tracing::debug!("[AUTHORIZE] Cleaned address: '{}'", address);

    tracing::debug!("[AUTHORIZE] Final parsed - address: '{}', worker: '{}', canxium: '{}'", address, worker_name, canxium_address);

    *ctx.wallet_addr.lock() = address.clone();
    *ctx.worker_name.lock() = worker_name.clone();

    if !canxium_address.is_empty() {
        *ctx.canxium_addr.lock() = canxium_address.clone();
    }

    let response = JsonRpcResponse::new(&event, Some(Value::Bool(true)), None);
    let response_json = serde_json::to_string(&response).unwrap_or_else(|_| "failed".to_string());
    tracing::debug!("[AUTHORIZE] Sending authorize response to {}: {}", ctx.remote_addr, response_json);

    ctx.reply(response).await.map_err(|e| format!("failed to send response to authorize: {}", e))?;

    tracing::debug!("[AUTHORIZE] Authorize response sent successfully");

    // CRITICAL: Message order for IceRiver must be:
    // 1. authorize response (done above)
    // 2. extranonce (if enabled) - MUST complete before difficulty/job
    // 3. difficulty
    // 4. job

    let extranonce = ctx.extranonce.lock().clone();
    if !extranonce.is_empty() {
        tracing::debug!("[AUTHORIZE] Step 2: Sending extranonce to client {} before difficulty/job", ctx.remote_addr);
        tracing::debug!("[AUTHORIZE] Extranonce value: '{}'", extranonce);
        send_extranonce(ctx.clone()).await?;
        tracing::debug!("[AUTHORIZE] Extranonce sent successfully to client {}", ctx.remote_addr);
    } else {
        tracing::debug!("[AUTHORIZE] No extranonce configured (extranonce_size=0), skipping extranonce step");
    }

    let wallet_addr = ctx.wallet_addr.lock().clone();
    let mut log_message = format!("[AUTHORIZE] Client authorized - address: {}", wallet_addr);
    if !canxium_address.is_empty() {
        log_message.push_str(&format!(", canxium address: {}", canxium_address));
    }
    tracing::debug!("{}", log_message);

    // CRITICAL: Send immediate job after authorization (IceRiver KS2L expects this)
    // Don't wait for polling loop - send job immediately
    // Difficulty will be sent inside send_immediate_job_to_client
    if let (Some(client_handler), Some(kaspa_api)) = (client_handler, kaspa_api) {
        tracing::debug!(
            "[AUTHORIZE] Step 3-4: Triggering immediate job send for client {} (extranonce already sent)",
            ctx.remote_addr
        );
        client_handler.send_immediate_job_to_client(ctx.clone(), kaspa_api).await;
    } else {
        // Fallback: let polling loop handle it (may cause disconnects for IceRiver)
        tracing::warn!("[AUTHORIZE] WARNING: No client_handler/kaspa_api available - job will be sent by polling loop (may cause IceRiver disconnect)");
    }

    tracing::debug!("[AUTHORIZE] ===== AUTHORIZE COMPLETE FOR {} =====", ctx.remote_addr);
    Ok(())
}

/// Handle submit request (stub - actual implementation in share_handler)
async fn handle_submit(ctx: Arc<StratumContext>, event: JsonRpcEvent) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing::debug!("[SUBMIT] ===== SUBMIT REQUEST FROM {} =====", ctx.remote_addr);
    tracing::debug!("[SUBMIT] Event ID: {:?}", event.id);
    tracing::debug!("[SUBMIT] Params count: {}", event.params.len());
    tracing::debug!("[SUBMIT] Full params: {:?}", event.params);
    tracing::debug!("[SUBMIT] Note: Actual processing happens in share_handler");
    Ok(())
}

/// Process Canxium address
fn process_canxium_address(address: &str) -> String {
    let mut addr = address.to_string();

    // Remove 0x prefix if present
    if addr.starts_with("0x") {
        addr = addr[2..].to_string();
    } else if addr.to_lowercase().starts_with("canxiuminer:0x") {
        // If it has both prefixes, remove the 0x part
        let prefix = &addr[.."canxiuminer:".len()];
        let address_part = &addr["canxiuminer:0x".len()..];
        addr = format!("{}{}", prefix, address_part);
    }

    // Make sure the address has the canxiuminer: prefix
    if !addr.to_lowercase().starts_with("canxiuminer:") {
        addr = format!("canxiuminer:{}", addr);
    }

    addr
}

/// Clean and validate wallet address
fn clean_wallet(input: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    // Try to decode as Kaspa address (supports kaspa:, kaspatest:, kaspadev:)
    if Address::try_from(input).is_ok() {
        return Ok(input.to_string());
    }

    // Try with kaspa: prefix if no recognized prefix
    if !input.starts_with("kaspa:") && !input.starts_with("kaspatest:") && !input.starts_with("kaspadev:") {
        return clean_wallet(&format!("kaspa:{}", input));
    }

    // Try regex match
    if let Some(captures) = WALLET_REGEX.find(input) {
        return Ok(captures.as_str().to_string());
    }

    Err("unable to coerce wallet to valid kaspa address".into())
}

/// Send extranonce to client
async fn send_extranonce(ctx: Arc<StratumContext>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing::debug!("[EXTRANONCE] ===== SENDING EXTRANONCE TO {} =====", ctx.remote_addr);

    let remote_app = ctx.remote_app.lock().clone();
    let extranonce = ctx.extranonce.lock().clone();

    tracing::debug!("[EXTRANONCE] Remote app: '{}', Extranonce: '{}'", remote_app, extranonce);

    // Bitmain requires extranonce2_size parameter - use same detection logic as assign_extranonce_for_miner
    // (case-insensitive matching for consistency)
    let is_bitmain_flag = crate::miner_detection::is_bitmain(&remote_app);
    tracing::debug!("[EXTRANONCE] Detected miner type - Remote app: '{}', Is Bitmain: {}", remote_app, is_bitmain_flag);

    let params = if is_bitmain_flag {
        let extranonce2_size = 8 - (extranonce.len() / 2);
        tracing::debug!("[EXTRANONCE] ===== USING BITMAIN EXTRANONCE FORMAT FOR {} =====", ctx.remote_addr);
        tracing::debug!(
            "[EXTRANONCE] Bitmain extranonce: '{}' ({} bytes), extranonce2_size: {} (calculated: 8 - {} / 2)",
            extranonce,
            extranonce.len() / 2,
            extranonce2_size,
            extranonce.len()
        );
        tracing::debug!("[EXTRANONCE] Bitmain params: ['{}', {}]", extranonce, extranonce2_size);
        vec![Value::String(extranonce.clone()), Value::Number(extranonce2_size.into())]
    } else {
        tracing::debug!("[EXTRANONCE] Using standard format (IceRiver/BzMiner)");
        vec![Value::String(extranonce.clone())]
    };

    // IceRiver expects minimal notification format (method + params only, no id or jsonrpc)
    // NOTE: This uses case-sensitive check to preserve exact existing behavior
    let is_iceriver_flag = crate::miner_detection::is_iceriver_case_sensitive(&remote_app);

    if is_iceriver_flag {
        tracing::debug!("[EXTRANONCE] Using minimal format for IceRiver (no id/jsonrpc)");
        ctx.send_notification("mining.set_extranonce", params.clone())
            .await
            .map_err(|e| format!("failed to set extranonce: {}", e))?;
    } else {
        // For non-IceRiver, use standard JSON-RPC format with jsonrpc field
        let event = JsonRpcEvent::new(None, "mining.set_extranonce", params.clone());
        let event_json = serde_json::to_string(&event).unwrap_or_else(|_| "failed".to_string());
        tracing::debug!("[EXTRANONCE] Sending mining.set_extranonce to {}: {}", ctx.remote_addr, event_json);
        ctx.send(event).await.map_err(|e| format!("failed to set extranonce: {}", e))?;
    }

    tracing::debug!("[EXTRANONCE] ===== EXTRANONCE SENT TO {} =====", ctx.remote_addr);
    Ok(())
}
