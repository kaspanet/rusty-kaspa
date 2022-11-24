use crate::protowire::rpc_server::RpcServer;
use kaspa_core::trace;
use rpc_core::server::service::RpcCoreService;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tonic::codec::CompressionEncoding;
use tonic::transport::{Error, Server};

pub mod connection;
pub mod service;

pub type StatusResult<T> = Result<T, tonic::Status>;

// TODO: use ctrl-c signaling infrastructure of kaspa-core

pub fn run_server(address: SocketAddr, core_service: Arc<RpcCoreService>) -> JoinHandle<Result<(), Error>> {
    trace!("KaspadRPCServer listening on: {}", address);

    let grpc_service = service::RpcService::new(core_service);
    grpc_service.start();

    let svc = RpcServer::new(grpc_service).send_compressed(CompressionEncoding::Gzip).accept_compressed(CompressionEncoding::Gzip);

    tokio::spawn(async move { Server::builder().add_service(svc).serve(address).await })
}
