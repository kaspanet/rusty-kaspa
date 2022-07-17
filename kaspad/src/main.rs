use std::sync::Arc;

use kaspa_core::*;
use kaspa_core::core::Core;

mod domain;

use domain::consensus::model::externalapi::hash::DomainHash;

pub fn main() {

    trace!("Kaspad starting...");
    

    let hash_str = "8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3af";
    let hash = DomainHash::from_string(&hash_str.to_owned());
    println!("{:?}", hash);

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

    trace!("Kaspad is finished...");

}

