use std::sync::Arc;

use kaspa_core::debug;
use p2p_lib::adaptor::P2pAdaptorApi;
use p2p_lib::infra::RouterApi;
use p2p_lib::infra::{self, P2pEvent};
use p2p_lib::registry::{EchoFlowRegistry, Flow};
use p2p_lib::{pb, registry};

fn main() {
    main2();
    std::thread::sleep(std::time::Duration::from_secs(20));
    debug!("P2P,p2p_client::main - EXITTTT");
}

#[tokio::main]
async fn main2() {
    // [-] - init logger
    kaspa_core::log::init_logger("debug");
    // [0] - init p2p-adaptor
    let registry = Arc::new(EchoFlowRegistry::new());
    let p2p_adaptor = p2p_lib::adaptor::P2pAdaptor::init_only_client_side(registry).await.unwrap();
    // [1] - connect 128 peers + flows
    let ip_port = String::from("://[::1]:16111");
    for i in 0..1 {
        debug!("P2P, p2p_client::main - starting peer:{}", i);
        let _peer_id = p2p_adaptor.connect_peer(ip_port.clone()).await;
    }
    // [2] - wait for 60 sec and terminate
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    debug!("P2P,p2p_client::main - TERMINATE");
    p2p_adaptor.terminate_all_peers_and_flows().await;
    debug!("P2P,p2p_client::main - FINISH");
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    debug!("P2P,p2p_client::main - EXIT");
}

#[allow(dead_code)]
async fn old_main_with_impl_details() {
    // [-] - init logger
    kaspa_core::log::init_logger("trace");
    // [0] - register first instance of router & channel to get new-routers when new connection established
    let (router, mut upper_layer_rx) = infra::Router::new().await;
    // [1] - Start service layer to listen when new connection is coming ( Server side )
    tokio::spawn(async move {
        // loop will exit when all sender channels will be dropped
        // --> when all routers will be dropped & grpc-service will be stopped
        while let Some(new_event) = upper_layer_rx.recv().await {
            if let P2pEvent::NewRouter(new_router) = new_event {
                // as en example subscribe to all message-types, in reality different flows will subscribe to different message-types
                let (_flow_id, flow_terminate) = registry::EchoFlow::new(new_router).await;
                // sleep for 30 sec
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                // terminate when needed
                flow_terminate.send(()).unwrap();
            }
        }
    });
    // [2] - Start client + re-connect loop
    let client = infra::P2pClient::connect_with_retry(String::from("://[::1]:50051"), router.clone(), false, 16).await;
    match client {
        Some(connected_client) => {
            // [2.*] - send message
            let msg = pb::KaspadMessage { payload: Some(pb::kaspad_message::Payload::Verack(pb::VerackMessage {})) };
            let result = connected_client.router.route_to_network(msg).await;
            if !result {
                panic!("Can't send message!!!");
            }
            // sleep for 30 sec
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            // [2.*] - close connection
            connected_client.router.as_ref().close().await;
        }
        None => {
            debug!("P2P, Client connection failed - 16 retries ...");
        }
    }
}
