use super::rpc_core_mock::RpcCoreMock;
use crate::adaptor::Adaptor;
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

    // Create and start a client
    debug!("Client 1 ==========================");
    let client1 = create_client(server.serve_address()).await;
    assert!(client1.disconnect().await.is_ok(), "error disconnecting the client");
    // Wait for disconnection completion
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Create and start a client
    debug!("Client 2 ==========================");
    let client2 = create_client(server.serve_address()).await;
    assert!(client2.disconnect().await.is_ok(), "error disconnecting the client");
    drop(client2);
    // Wait for disconnection completion
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Create and start a client
    debug!("Client 3 ==========================");
    let _client3 = create_client(server.serve_address()).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Stop the server
    assert!(server.terminate().await.is_ok(), "error stopping the server");
    drop(server);

    // Wait for server termination
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
}

fn create_server() -> Arc<Adaptor> {
    let core_service = Arc::new(RpcCoreMock {});
    let core_notifier: Arc<Notifier<Notification, ChannelConnection>> =
        Arc::new(Notifier::new(EVENT_TYPE_ARRAY[..].into(), vec![], vec![], 1, "rpc-core"));
    Adaptor::server(get_free_net_address(), core_service, core_notifier)
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
