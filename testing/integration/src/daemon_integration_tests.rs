use crate::common::daemon::Daemon;
use kaspa_core::signals::Shutdown;
use std::time::Duration;

#[tokio::test]
async fn daemon_sanity_test() {
    let core1 = Daemon::new_random();
    let (workers1, rpc_client1) = core1.start().await;

    let core2 = Daemon::new_random();
    let (workers2, rpc_client2) = core2.start().await;

    tokio::time::sleep(Duration::from_secs(1)).await;
    rpc_client1.disconnect().await.unwrap();
    drop(rpc_client1);
    core1.core.shutdown();
    core1.core.join(workers1);

    rpc_client2.disconnect().await.unwrap();
    drop(rpc_client2);
    core2.core.shutdown();
    core2.core.join(workers2);
}
