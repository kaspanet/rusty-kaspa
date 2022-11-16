use crate::protowire::rpc_server::RpcServer;
use rpc_core::server::service::RpcApi;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tonic::codec::CompressionEncoding;
use tonic::transport::{Error, Server};

pub mod connection;
pub mod service;

pub type StatusResult<T> = Result<T, tonic::Status>;

// TODO: use ctrl-c signaling infrastructure of kaspa-core

// see https://hyper.rs/guides/server/graceful-shutdown/
// async fn shutdown_signal() {

//     // Wait for the CTRL+C signal
//     //tokio::signal::ctrl_c().await.expect("failed to install CTRL+C signal handler");
// }

pub fn run_server(address: SocketAddr, core_service: Arc<RpcApi>) -> JoinHandle<Result<(), Error>> {
    println!("KaspadRPCServer listening on: {}", address);

    let grpc_service = service::RpcService::new(core_service);
    grpc_service.start();

    let svc = RpcServer::new(grpc_service).send_compressed(CompressionEncoding::Gzip).accept_compressed(CompressionEncoding::Gzip);

    tokio::spawn(async move { Server::builder().add_service(svc).serve(address).await })
    //tokio::spawn(async move { Server::builder().add_service(svc).serve_with_shutdown(address, shutdown_signal()).await })
}
