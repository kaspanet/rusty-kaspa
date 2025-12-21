use crate::{
    client_handler::ClientHandler,
    default_client::*,
    jsonrpc_event::JsonRpcEvent,
    kaspaapi::KaspaApi,
    share_handler::{KaspaApiTrait, ShareHandler},
    stratum_context::StratumContext,
    stratum_listener::{StratumListener, StratumListenerConfig},
};
use std::sync::Arc;
use std::time::Duration;
use tracing::warn;

pub struct BridgeConfig {
    pub instance_id: String, // Instance identifier for logging (e.g., "Instance 1", "Instance 2")
    pub stratum_port: String,
    pub kaspad_address: String,
    pub prom_port: String,
    pub print_stats: bool,
    pub log_to_file: bool,
    pub health_check_port: String,
    pub block_wait_time: Duration,
    pub min_share_diff: u32,
    pub var_diff: bool,
    pub shares_per_min: u32,
    pub var_diff_stats: bool,
    pub extranonce_size: u8,
    pub pow2_clamp: bool,
}

/// Start block template listener with concrete KaspaApi
/// This should be called from main.rs where we have concrete type
pub async fn start_block_template_listener_with_api(
    kaspa_api: Arc<KaspaApi>,
    block_wait_time: Duration,
    client_handler: Arc<ClientHandler>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client_handler_cb = Arc::clone(&client_handler);
    let kaspa_api_cb = Arc::clone(&kaspa_api);

    let block_cb = move || {
        let client_handler = Arc::clone(&client_handler_cb);
        let kaspa_api = Arc::clone(&kaspa_api_cb);
        tokio::spawn(async move {
            client_handler.new_block_available(kaspa_api).await;
        });
    };

    kaspa_api
        .start_block_template_listener(block_wait_time, block_cb)
        .await
        .map_err(|e| Box::new(std::io::Error::other(e.to_string())) as Box<dyn std::error::Error + Send + Sync>)
}

pub async fn listen_and_serve<T: KaspaApiTrait + Send + Sync + 'static>(
    config: BridgeConfig,
    kaspa_api: Arc<T>,
    // Optional: if concrete KaspaApi is provided, use notification-based listener
    concrete_kaspa_api: Option<Arc<KaspaApi>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Calculate min diff with pow2 clamp if needed
    let mut min_diff = config.min_share_diff as f64;
    if config.pow2_clamp && min_diff > 0.0 {
        min_diff = 2_f64.powi((min_diff.log2().floor()) as i32);
    }
    if min_diff == 0.0 {
        min_diff = 4.0;
    }

    // Extranonce size is now auto-detected per client based on miner type
    // We still need to pass a value to ClientHandler::new() for backward compatibility,
    // but it will be ignored as extranonce is assigned per-client in handle_subscribe
    // Default to 2 (for IceRiver/BzMiner/Goldshell) as that's the most common case
    let extranonce_size = if config.extranonce_size > 0 {
        config.extranonce_size.min(3) as i8
    } else {
        2 // Default to 2, will be auto-detected per client anyway
    };

    // Create share handler with instance identifier
    let instance_id = config.instance_id.clone();
    let share_handler = Arc::new(ShareHandler::new(instance_id.clone()));

    // Create client handler
    // Note: extranonce_size parameter is now only used for backward compatibility
    // Actual extranonce assignment happens per-client in handle_subscribe based on detected miner type
    let client_handler = Arc::new(ClientHandler::new(Arc::clone(&share_handler), min_diff, extranonce_size, instance_id.clone()));

    // Setup default handlers
    let mut handlers = default_handlers();

    // Override subscribe handler to enable automatic extranonce detection
    let subscribe_handler = {
        let client_handler = Arc::clone(&client_handler);
        Arc::new(move |ctx: Arc<StratumContext>, event: JsonRpcEvent| {
            let client_handler = Arc::clone(&client_handler);
            let ctx_clone = Arc::clone(&ctx);
            let event_clone = event.clone();
            Box::pin(async move {
                crate::default_client::handle_subscribe(ctx_clone, event_clone, Some(client_handler))
                    .await
                    .map_err(|e| Box::new(std::io::Error::other(e.to_string())) as Box<dyn std::error::Error + Send + Sync>)
            })
                as std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send>>
        }) as crate::stratum_listener::EventHandler
    };
    handlers.insert("mining.subscribe".to_string(), subscribe_handler);

    // Override authorize handler to send immediate job (critical for IceRiver KS2L)
    let authorize_handler = {
        let client_handler = Arc::clone(&client_handler);
        let kaspa_api = Arc::clone(&kaspa_api);
        Arc::new(move |ctx: Arc<StratumContext>, event: JsonRpcEvent| {
            let client_handler = Arc::clone(&client_handler);
            let kaspa_api = Arc::clone(&kaspa_api);
            let ctx_clone = Arc::clone(&ctx);
            let event_clone = event.clone();
            Box::pin(async move {
                crate::default_client::handle_authorize(ctx_clone, event_clone, Some(client_handler), Some(kaspa_api))
                    .await
                    .map_err(|e| Box::new(std::io::Error::other(e.to_string())) as Box<dyn std::error::Error + Send + Sync>)
            })
                as std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send>>
        }) as crate::stratum_listener::EventHandler
    };
    handlers.insert("mining.authorize".to_string(), authorize_handler);

    // Override submit handler
    let submit_handler = {
        let share_handler = Arc::clone(&share_handler);
        let kaspa_api = Arc::clone(&kaspa_api);
        Arc::new(move |ctx: Arc<StratumContext>, event: JsonRpcEvent| {
            let share_handler = Arc::clone(&share_handler);
            let kaspa_api = Arc::clone(&kaspa_api);
            let ctx_clone = Arc::clone(&ctx);
            Box::pin(async move {
                share_handler
                    .handle_submit(ctx_clone, event, kaspa_api)
                    .await
                    .map_err(|e| Box::new(std::io::Error::other(e.to_string())) as Box<dyn std::error::Error + Send + Sync>)
            })
                as std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send>>
        }) as crate::stratum_listener::EventHandler
    };
    handlers.insert("mining.submit".to_string(), submit_handler);

    // Setup listener config
    // Each client will get its own MiningState (created in stratum_listener)
    // Each client gets its own isolated state
    let listener_config = StratumListenerConfig {
        port: config.stratum_port.clone(),
        handler_map: Arc::new(handlers),
        on_connect: Arc::new({
            let client_handler = Arc::clone(&client_handler);
            move |ctx: Arc<StratumContext>| {
                client_handler.on_connect(ctx);
            }
        }),
        on_disconnect: Arc::new({
            let client_handler = Arc::clone(&client_handler);
            move |ctx: Arc<StratumContext>| {
                client_handler.on_disconnect(&ctx);
            }
        }),
    };

    // Start vardiff thread if enabled
    if config.var_diff {
        let shares_per_min = if config.shares_per_min > 0 { config.shares_per_min } else { 20 };

        // Expose target shares-per-minute to the stats printer so it can
        // display VarDiff columns (diff / spm / target / trend / status)
        share_handler.set_target_spm(shares_per_min as f64);
        share_handler.start_vardiff_thread(shares_per_min, config.var_diff_stats, config.pow2_clamp);
    }

    // Start stats printing thread if enabled
    if config.print_stats {
        share_handler.start_print_stats_thread();
    }

    // Start stats pruning thread
    share_handler.start_prune_stats_thread();

    // Start block template listener with notifications + ticker fallback
    // This provides immediate notifications when new blocks are available, with polling as fallback

    // If concrete KaspaApi is provided, use notification-based listener
    // Otherwise, use polling only (fallback for trait objects)
    if let Some(concrete_api) = concrete_kaspa_api {
        // We have concrete KaspaApi - use notification-based listener
        let client_handler_cb = Arc::clone(&client_handler);
        let kaspa_api_cb = Arc::clone(&kaspa_api);

        let block_cb = move || {
            let client_handler = Arc::clone(&client_handler_cb);
            let kaspa_api = Arc::clone(&kaspa_api_cb);
            tokio::spawn(async move {
                client_handler.new_block_available(kaspa_api).await;
            });
        };

        // Start notification-based listener with ticker fallback
        // Method signature: start_block_template_listener(self: Arc<Self>, ...)
        // Call the method directly on Arc<KaspaApi> (it's an instance method taking Arc<Self>)
        if let Err(e) = concrete_api.start_block_template_listener(config.block_wait_time, block_cb).await {
            warn!("Failed to start notification-based block template listener: {}, falling back to polling", e);
            // Fall through to polling approach
        } else {
            // Successfully started notification-based listener
            tracing::debug!("Started notification-based block template listener");
        }
    } else {
        // No concrete KaspaApi provided - use polling only
        warn!("Using polling-based block template listener (concrete KaspaApi not provided, notifications not available)");

        let client_handler_poll = Arc::clone(&client_handler);
        let kaspa_api_poll = Arc::clone(&kaspa_api);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.block_wait_time);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                // Poll for new blocks
                client_handler_poll.new_block_available(Arc::clone(&kaspa_api_poll)).await;
            }
        });
    }

    // Start listener
    let listener = StratumListener::new(listener_config);
    tracing::info!("{} Starting stratum listener on {}", instance_id, config.stratum_port);
    listener.listen().await
}
