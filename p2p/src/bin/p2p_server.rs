use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use kaspa_core::{debug, error};
use kaspa_p2p_lib::kaspa_flows;
use kaspa_p2p_lib::kaspa_flows::Flow;
use kaspa_p2p_lib::kaspa_grpc;
use kaspa_p2p_lib::kaspa_grpc::RouterApi;
use kaspa_p2p_lib::kaspa_p2p::P2pAdaptorApi;

#[tokio::main]
async fn main() {
    // [-] - init logger
    kaspa_core::log::init_logger("trace");
    // [0] - init p2p-adaptor - server side
    let ip_port = String::from("[::1]:50051");
    let p2p_adaptor = kaspa_p2p_lib::kaspa_p2p::P2pAdaptor::listen(ip_port.clone()).await.unwrap();
    // [1] - wait for 60 sec & terminate
    std::thread::sleep(std::time::Duration::from_secs(128));
    debug!("P2P, p2p_server::main - TERMINATE");
    p2p_adaptor.terminate_all_peers_and_flows().await;
    // [2] - drop & sleep 5 sec
    drop(p2p_adaptor);
    std::thread::sleep(std::time::Duration::from_secs(5));
    debug!("P2P, p2p_server::main - FINISH");
}
#[allow(dead_code)]
async fn old_main_with_impl_details() {
    // [-] - init logger
    kaspa_core::log::init_logger("trace");
    // [0] - Create new router - first instance
    // upper_layer_rx will be used to dispatch notifications about new-connections, both for client & server
    let (router, mut upper_layer_rx) = kaspa_grpc::Router::new().await;
    // [1] - Start service layer to listen when new connection is coming ( Server side )
    tokio::spawn(async move {
        // loop will exit when all sender channels will be dropped
        // --> when all routers will be dropped & grpc-service will be stopped
        while let Some(new_router) = upper_layer_rx.recv().await {
            // as en example subscribe to all message-types, in reality different flows will subscribe to different message-types
            let flow_terminate = kaspa_flows::EchoFlow::new(new_router).await;
            // sleep for 30 sec
            std::thread::sleep(std::time::Duration::from_secs(30));
            // terminate when needed (as an example) in general we need to save it somewhere in order to do graceful shutdown
            flow_terminate.send(()).unwrap();
        }
    });
    // [2] - Start listener (de-facto Server side )
    let terminate_server = kaspa_grpc::P2pServer::listen(String::from("[::1]:50051"), router, true).await;
    let terminate_signal = Arc::new(AtomicBool::new(false));

    // [3] - Check that server is ok & register termination signal ( as an example )
    match terminate_server {
        Ok(sender) => {
            debug!("P2P, Server is running ... & we can terminate it with CTRL-C");
            let terminate_clone = terminate_signal.clone();
            ctrlc::set_handler(move || {
                terminate_clone.store(true, Ordering::SeqCst);
            })
            .unwrap();
            // [4] - sleep - just not to exit main function
            debug!("P2P, Server-side, endless sleep....");
            loop {
                if terminate_signal.load(Ordering::SeqCst) {
                    debug!("P2P, Received termination signal");
                    // terminate grpc service
                    sender.send(()).unwrap();
                    break;
                }
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
        Err(err) => {
            error!("P2P, Server can't start, {:?}", err);
        }
    }
}
