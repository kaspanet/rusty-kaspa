use std::sync::Arc;

use kaspa_core::*;
use kaspa_core::core::Core;

pub fn main() {

    trace!("Kaspa Wallet starting...");
    
    let core = Arc::new(Core::new());
    let signals = Arc::new(signals::Signals::new(core.clone()));
    signals.init();


    let monitor = Arc::new(monitor::Monitor::new());
    let test_service_a = Arc::new(test_service::TestService::new("test servivce A"));
    let test_service_b = Arc::new(test_service::TestService::new("test servivce B"));

    // signals.bind(&core);
    core.bind(monitor.clone());
    core.bind(test_service_a.clone());
    core.bind(test_service_b.clone());

    core.run();

    trace!("Kaspa wallet is finished...");

}

