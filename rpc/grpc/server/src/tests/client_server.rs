use super::rpc_core_mock::RpcCoreMock;
use crate::connection_handler::GrpcConnectionHandler;
use kaspa_core::debug;
use kaspa_grpc_client::GrpcClient;
use kaspa_notify::{events::EVENT_TYPE_ARRAY, notifier::Notifier};
use kaspa_rpc_core::notify::mode::NotificationMode;
use kaspa_rpc_core::{notify::connection::ChannelConnection, Notification};
use kaspa_utils::networking::{ContextualNetAddress, NetAddress};
use std::sync::Arc;

#[tokio::test]
async fn test_server_client() {
    kaspa_core::log::try_init_logger("info, kaspa_grpc_core=trace, kaspa_grpc_server=trace, kaspa_grpc_client=trace");

    // Create and start the server
    let server = create_server();
    let server_address = get_free_net_address();
    let _shutdown_signal = server.serve(server_address);
    server.start();

    // Create and start a client
    debug!("Client 1 ==========================");
    let client = create_client(server_address).await;
    assert!(client.disconnect().await.is_ok(), "error disconnecting the client");
    // Wait for disconnection completion
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Create and start a client
    debug!("Client 2 ==========================");
    let client = create_client(server_address).await;
    assert!(client.disconnect().await.is_ok(), "error disconnecting the client");
    // Wait for disconnection completion
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Stop the server
    assert!(server.stop().await.is_ok(), "error stopping the server");
}

fn create_server() -> Arc<GrpcConnectionHandler> {
    let core_service = Arc::new(RpcCoreMock {});
    let core_notifier: Arc<Notifier<Notification, ChannelConnection>> =
        Arc::new(Notifier::new(EVENT_TYPE_ARRAY[..].into(), vec![], vec![], 1, "rpc-core"));
    Arc::new(GrpcConnectionHandler::new(core_service, core_notifier))
}

async fn create_client(server_address: NetAddress) -> GrpcClient {
    let server_url = format!("grpc://localhost:{}", server_address.port);
    GrpcClient::connect(NotificationMode::Direct, server_url, false, None, false, None).await.unwrap()
}

fn get_free_net_address() -> NetAddress {
    let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = socket.local_addr().unwrap().port();

    drop(socket);
    ContextualNetAddress::unspecified().normalize(port)
}
