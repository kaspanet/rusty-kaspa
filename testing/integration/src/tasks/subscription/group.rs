use crate::{
    common::daemon::ClientManager,
    tasks::{
        notify::notification_drainer::NotificationDrainerTask,
        subscription::{
            address_subscriber::AddressSubscriberTask, basic_subscriber::BasicSubscriberTask, submitter::SubscriptionSubmitterTask,
        },
        Task,
    },
};
use async_trait::async_trait;
use itertools::{chain, Itertools};
use kaspa_addresses::Address;
use kaspa_notify::scope::Scope;
use kaspa_utils::triggers::SingleTrigger;
use std::sync::Arc;
use tokio::task::JoinHandle;

pub struct SubscriberGroupTask {
    submitter: Arc<SubscriptionSubmitterTask>,
    basic_subscriber: Arc<BasicSubscriberTask>,
    address_subscriber: Arc<AddressSubscriberTask>,
    notification_drainer: Arc<NotificationDrainerTask>,
}

impl SubscriberGroupTask {
    pub fn new(
        submitter: Arc<SubscriptionSubmitterTask>,
        basic_subscriber: Arc<BasicSubscriberTask>,
        address_subscriber: Arc<AddressSubscriberTask>,
        notification_drainer: Arc<NotificationDrainerTask>,
    ) -> Self {
        Self { submitter, basic_subscriber, address_subscriber, notification_drainer }
    }

    pub async fn build(
        client_manager: Arc<ClientManager>,
        workers: usize,
        bps: u64,
        basic_subscriptions: Vec<Scope>,
        basic_initial_secs_delay: u64,
        addresses: Vec<Arc<Vec<Address>>>,
        address_initial_secs_delay: u64,
        address_cycle_seconds: u64,
        address_max_cycles: usize,
    ) -> Arc<Self> {
        // Clients
        assert!(!addresses.is_empty());
        let clients = client_manager.new_clients(addresses.len()).await.into_iter().map(Arc::new).collect_vec();

        // Block submitter
        let submitter = SubscriptionSubmitterTask::build(workers, addresses.len(), bps);

        // Basic subscriber
        let basic_subscriber =
            BasicSubscriberTask::build(clients.clone(), basic_subscriptions, submitter.sender(), basic_initial_secs_delay);

        // Address subscriber
        let address_subscriber = AddressSubscriberTask::build(
            clients.clone(),
            addresses,
            submitter.sender(),
            address_initial_secs_delay,
            address_cycle_seconds,
            address_max_cycles,
        );

        // Notification drainer
        let notification_drainer = NotificationDrainerTask::build(clients);

        Arc::new(Self::new(submitter, basic_subscriber, address_subscriber, notification_drainer))
    }
}

#[async_trait]
impl Task for SubscriberGroupTask {
    fn start(&self, stop_signal: SingleTrigger) -> Vec<JoinHandle<()>> {
        chain![
            self.submitter.start(stop_signal.clone()),
            self.basic_subscriber.start(stop_signal.clone()),
            self.address_subscriber.start(stop_signal.clone()),
            self.notification_drainer.start(stop_signal.clone()),
        ]
        .collect()
    }
}
