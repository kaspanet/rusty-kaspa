use super::rpc_core_mock::RpcCoreMock;
use crate::adaptor::Adaptor;
use kaspa_core::info;
use kaspa_grpc_client::GrpcClient;
use kaspa_rpc_core::notify::mode::NotificationMode;
use kaspa_utils::networking::{ContextualNetAddress, NetAddress};
use kaspa_utils::tcp_limiter::Limit;
use std::sync::Arc;

#[tokio::test]
async fn test_client_server_connections() {
    kaspa_core::log::try_init_logger("info, kaspa_grpc_core=trace, kaspa_grpc_server=trace, kaspa_grpc_client=trace");

    // Create and start a fake core service
    let core_service = Arc::new(RpcCoreMock::new());
    core_service.start();

    // Create and start the server
    let server = create_server(core_service.clone());
    assert!(!server.has_connections(), "server should have no client when just started");

    info!("=================================================================================");
    info!("2 clients connecting and disconnecting themselves");

    let client1 = create_client(server.serve_address()).await;
    let client2 = create_client(server.serve_address()).await;

    assert_eq!(server.active_connections().len(), 2, "one or more clients failed to connect to the server");

    assert!(client1.disconnect().await.is_ok(), "client 1 failed to disconnect");
    assert!(client2.disconnect().await.is_ok(), "client 2 failed to disconnect");

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(!server.has_connections(), "server should have no more clients");

    info!("=================================================================================");
    info!("2 clients connecting and server disconnecting them");

    let client1 = create_client(server.serve_address()).await;
    let client2 = create_client(server.serve_address()).await;

    assert_eq!(server.active_connections().len(), 2, "one or more clients failed to connect to the server");

    server.terminate_all_connections();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    assert!(!client1.is_connected(), "server failed to disconnect client 1");
    assert!(!client2.is_connected(), "server failed to disconnect client 2");
    assert!(!server.has_connections(), "server should have no more clients");

    info!("=================================================================================");
    info!("2 clients connecting, 1 disconnecting itself, server shutting down");

    let client1 = create_client(server.serve_address()).await;
    let client2 = create_client(server.serve_address()).await;

    assert_eq!(server.active_connections().len(), 2, "one or more clients failed to connect to the server");

    assert!(client1.disconnect().await.is_ok(), "client 1 failed to disconnect");
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(server.active_connections().len(), 1, "server should have one client left connected");

    // Stop the fake service
    core_service.join().await;

    // Stop the server
    assert!(server.terminate().await.is_ok(), "error stopping the server");

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(!client2.is_connected(), "server failed to disconnect client 2");
    assert!(!server.has_connections(), "server should have no more clients");

    drop(server);

    // Wait for server termination (just for logging properly)
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
}

#[tokio::test]
async fn test_client_server_connections_tcp_conn_limit() {
    kaspa_core::log::try_init_logger("info, kaspa_grpc_core=trace, kaspa_grpc_server=trace, kaspa_grpc_client, kaspa_utils =trace");

    // Create and start a fake core service
    let core_service = Arc::new(RpcCoreMock::new());
    core_service.start();

    // Create and start the server
    let server = create_server_with_limit(core_service.clone(), Limit::new(2));
    assert!(!server.has_connections(), "server should have no client when just started");

    let client1 = create_client(server.serve_address()).await;
    let client2 = create_client(server.serve_address()).await;

    assert_eq!(server.active_connections().len(), 2, "one or more clients failed to connect to the server");
    let address = server.serve_address();
    tokio::spawn(async move {
        let client3 = create_client(address).await;
        assert!(client3.disconnect().await.is_ok(), "client 3 failed to disconnect");
    });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    assert_eq!(server.active_connections().len(), 2, "limit doesn't work");

    assert!(client1.disconnect().await.is_ok(), "client 1 failed to disconnect");
    assert!(client2.disconnect().await.is_ok(), "client 2 failed to disconnect");

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(!server.has_connections(), "server should have no more clients");
    // Stop the fake service
    core_service.join().await;

    // Stop the server
    assert!(server.terminate().await.is_ok(), "error stopping the server");

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(!client2.is_connected(), "server failed to disconnect client 2");
    assert!(!server.has_connections(), "server should have no more clients");

    drop(server);

    // Wait for server termination (just for logging properly)
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
}

fn create_server(core_service: Arc<RpcCoreMock>) -> Arc<Adaptor> {
    Adaptor::server(get_free_net_address(), core_service.clone(), core_service.core_notifier(), 128, None)
}

fn create_server_with_limit(core_service: Arc<RpcCoreMock>, tcp_limit: impl Into<Arc<Limit>>) -> Arc<Adaptor> {
    Adaptor::server(get_free_net_address(), core_service.clone(), core_service.core_notifier(), 128, Some(tcp_limit.into()))
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
