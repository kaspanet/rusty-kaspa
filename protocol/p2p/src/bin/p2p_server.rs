use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use kaspa_core::{debug, error};
use p2p_lib::adaptor::P2pAdaptorApi;
use p2p_lib::infra;
use p2p_lib::infra::RouterApi;
use p2p_lib::registry;
use p2p_lib::registry::{EchoFlowRegistry, Flow};

#[tokio::main]
async fn main() {
    // [-] - init logger
    kaspa_core::log::init_logger("info");
    // [0] - init p2p-adaptor - server side
    let ip_port = String::from("[::1]:50051");
    let registry = Arc::new(EchoFlowRegistry::new());
    let p2p_adaptor = p2p_lib::adaptor::P2pAdaptor::listen(ip_port.clone(), registry).await.unwrap();

    let other_ip_port = String::from("http://[::1]:16111");
    for i in 0..1 {
        debug!("P2P, p2p::main - starting peer:{}", i);
        let _peer_id = p2p_adaptor.connect_peer(other_ip_port.clone()).await;
        // let msg = pb::KaspadMessage { payload: Some(pb::kaspad_message::Payload::Verack(pb::VerackMessage {})) };
        // p2p_adaptor.send(peer_id.unwrap(), msg).await;
    }

    // [1] - wait for a few sec & terminate
    tokio::time::sleep(std::time::Duration::from_secs(64)).await;
    debug!("P2P, p2p_server::main - TERMINATE");
    p2p_adaptor.terminate_all_peers_and_flows().await;
    // [2] - drop & sleep 5 sec
    drop(p2p_adaptor);
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    debug!("P2P, p2p_server::main - FINISH");
}

#[allow(dead_code)]
async fn old_main_with_impl_details() {
    // [-] - init logger
    kaspa_core::log::init_logger("trace");
    // [0] - Create new router - first instance
    // upper_layer_rx will be used to dispatch notifications about new-connections, both for client & server
    let (router, mut upper_layer_rx) = infra::Router::new().await;
    // [1] - Start service layer to listen when new connection is coming ( Server side )
    tokio::spawn(async move {
        // loop will exit when all sender channels will be dropped
        // --> when all routers will be dropped & grpc-service will be stopped
        while let Some(new_router) = upper_layer_rx.recv().await {
            // as en example subscribe to all message-types, in reality different flows will subscribe to different message-types
            let (_flow_id, flow_terminate) = registry::EchoFlow::new(new_router).await;
            // sleep for 30 sec
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            // terminate when needed (as an example) in general we need to save it somewhere in order to do graceful shutdown
            flow_terminate.send(()).unwrap();
        }
    });
    // [2] - Start listener (de-facto Server side )
    let terminate_server = infra::P2pServer::listen(String::from("[::1]:50051"), router, true).await;
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
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
        Err(err) => {
            error!("P2P, Server can't start, {:?}", err);
        }
    }
}
