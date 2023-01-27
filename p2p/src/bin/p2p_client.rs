use kaspa_core::debug;
use kaspa_p2p_lib::kaspa_flows::Flow;
use kaspa_p2p_lib::kaspa_grpc;
use kaspa_p2p_lib::kaspa_grpc::RouterApi;
use kaspa_p2p_lib::kaspa_p2p::P2pAdaptorApi;
use kaspa_p2p_lib::{kaspa_flows, pb};

#[tokio::main]
async fn main() {
    // [-] - init logger
    kaspa_core::log::init_logger("trace");
    // [0] - init p2p-adaptor
    let p2p_adaptor = kaspa_p2p_lib::kaspa_p2p::P2pAdaptor::init_only_client_side().await.unwrap();
    // [1] - connect 128 peers + flows
    let ip_port = String::from("http://[::1]:50051");
    for i in 0..1024 {
        debug!("P2P, p2p_client::main - starting peer:{}", i);
        let peer_id = p2p_adaptor.connect_peer(ip_port.clone()).await;
        let msg = pb::KaspadMessage { payload: Some(pb::kaspad_message::Payload::Verack(pb::VerackMessage {})) };
        p2p_adaptor.send(peer_id.unwrap(), msg).await;
    }
    // [2] - wait for 60 sec and terminate
    std::thread::sleep(std::time::Duration::from_secs(128));
    debug!("P2P,p2p_client::main - TERMINATE");
    p2p_adaptor.terminate_all_peers_and_flows().await;
    debug!("P2P,p2p_client::main - FINISH");
}
#[allow(dead_code)]
async fn old_main_with_impl_details() {
    // [-] - init logger
    kaspa_core::log::init_logger("trace");
    // [0] - register first instance of router & channel to get new-routers when new connection established
    let (router, mut upper_layer_rx) = kaspa_grpc::Router::new().await;
    // [1] - Start service layer to listen when new connection is coming ( Server side )
    tokio::spawn(async move {
        // loop will exit when all sender channels will be dropped
        // --> when all routers will be dropped & grpc-service will be stopped
        while let Some(new_router) = upper_layer_rx.recv().await {
            // as en example subscribe to all message-types, in reality different flows will subscribe to different message-types
            // as en example subscribe to all message-types, in reality different flows will subscribe to different message-types
            let flow_terminate = kaspa_flows::EchoFlow::new(new_router).await;
            // sleep for 30 sec
            std::thread::sleep(std::time::Duration::from_secs(30));
            // terminate when needed
            flow_terminate.send(()).unwrap();
        }
    });
    // [2] - Start client + re-connect loop
    let client = kaspa_grpc::P2pClient::connect_with_retry(String::from("http://[::1]:50051"), router.clone(), false, 16).await;
    match client {
        Some(connected_client) => {
            // [2.*] - send message
            let msg = pb::KaspadMessage { payload: Some(pb::kaspad_message::Payload::Verack(pb::VerackMessage {})) };
            let result = connected_client.router.route_to_network(msg).await;
            if !result {
                panic!("Can't send message!!!");
            }
            // sleep for 30 sec
            std::thread::sleep(std::time::Duration::from_secs(30));
            // [2.*] - close connection
            connected_client.router.as_ref().close().await;
        }
        None => {
            debug!("P2P, Client connection failed - 16 retries ...");
        }
    }
    /*
    let cloned_router_arc = router.clone();
    let mut cnt = 0;
    loop {
        let client = kaspa_grpc::P2pClient::connect(String::from("http://[::1]:50051"), cloned_router_arc.clone(), false).await;
        if client.is_ok() {
            println!("Client connected ... we can terminate ...");
            client.unwrap().router.as_ref().close().await;
        } else {
            println!("{:?}", client.err());
            cnt = cnt + 1;
            if cnt > 320 {
                println!("Client connected failed - 16 retries ...");
                break;
            } else {
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
        }
    }
    */
}
