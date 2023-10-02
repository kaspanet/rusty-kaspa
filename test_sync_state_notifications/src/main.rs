use kaspa_core::{error, info};
use kaspa_grpc_client::GrpcClient;
use kaspa_notify::scope::{Scope, SyncStateChangedScope};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::notify::mode::NotificationMode;
use kaspa_wrpc_client::{KaspaRpcClient, WrpcEncoding};

#[tokio::main]
async fn main() {
    kaspa_core::log::try_init_logger("info");

    // let client = GrpcClient::connect(NotificationMode::Direct, "grpc://127.0.0.1:16210".to_string(), true, None, false, Some(100_000))
    //     .await
    //     .unwrap();
    let client = KaspaRpcClient::new(WrpcEncoding::SerdeJson, "ws://127.0.0.1:17210".into()).unwrap();
    client.connect(Default::default()).await.unwrap();
    client.start().await.unwrap();

    client.start_notify(1, Scope::SyncStateChanged { 0: SyncStateChangedScope {} }).await.unwrap();
    let receiver = client.notification_channel_receiver();
    loop {
        match receiver.recv().await {
            Ok(n) => info!("{n:?}"),
            Err(e) => error!("{e:?}"),
        }
    }
}
