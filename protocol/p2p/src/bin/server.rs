use kaspa_core::debug;
use kaspa_p2p_lib::echo::EchoFlowInitializer;
use std::{sync::Arc, time::Duration};

#[tokio::main]
async fn main() {
    // [-] - init logger
    kaspa_core::log::init_logger("debug");
    // [0] - init p2p-adaptor - server side
    let ip_port = String::from("[::1]:50051");
    let initializer = Arc::new(EchoFlowInitializer::new());
    let adaptor = kaspa_p2p_lib::Adaptor::bidirectional(ip_port, initializer).unwrap();
    // [1] - connect to a few peers
    let ip_port = String::from("[::1]:16111");
    for i in 0..1 {
        debug!("P2P, p2p_client::main - starting peer:{}", i);
        let _peer_id = adaptor.connect_peer_with_retries(ip_port.clone(), 16, Duration::from_secs(1)).await;
    }
    // [2] - wait for ~60 sec and terminate
    tokio::time::sleep(Duration::from_secs(64)).await;
    debug!("P2P,p2p_client::main - TERMINATE");
    adaptor.terminate_all_peers().await;
    debug!("P2P,p2p_client::main - FINISH");
    tokio::time::sleep(Duration::from_secs(10)).await;
    debug!("P2P,p2p_client::main - EXIT");
}
