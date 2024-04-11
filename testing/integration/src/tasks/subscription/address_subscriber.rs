use crate::tasks::{subscription::submitter::SubscribeCommand, Task};
use async_channel::Sender;
use async_trait::async_trait;
use kaspa_addresses::Address;
use kaspa_core::warn;
use kaspa_grpc_client::GrpcClient;
use kaspa_utils::triggers::SingleTrigger;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::oneshot::channel, task::JoinHandle, time::sleep};

pub struct AddressSubscriberTask {
    clients: Vec<Arc<GrpcClient>>,
    addresses: Vec<Arc<Vec<Address>>>,
    command_sender: Sender<SubscribeCommand>,
    initial_secs_delay: u64,
    cycle_seconds: u64,
    max_cycles: usize,
}

impl AddressSubscriberTask {
    pub fn new(
        clients: Vec<Arc<GrpcClient>>,
        addresses: Vec<Arc<Vec<Address>>>,
        command_sender: Sender<SubscribeCommand>,
        initial_secs_delay: u64,
        cycle_seconds: u64,
        max_cycles: usize,
    ) -> Self {
        assert_eq!(clients.len(), addresses.len());
        Self { clients, addresses, command_sender, initial_secs_delay, cycle_seconds, max_cycles }
    }

    pub fn build(
        clients: Vec<Arc<GrpcClient>>,
        addresses: Vec<Arc<Vec<Address>>>,
        command_sender: Sender<SubscribeCommand>,
        initial_secs_delay: u64,
        cycle_seconds: u64,
        max_cycles: usize,
    ) -> Arc<Self> {
        Arc::new(Self::new(clients, addresses, command_sender, initial_secs_delay, cycle_seconds, max_cycles))
    }

    pub fn clients(&self) -> &[Arc<GrpcClient>] {
        &self.clients
    }
}

#[async_trait]
impl Task for AddressSubscriberTask {
    fn start(&self, stop_signal: SingleTrigger) -> Vec<JoinHandle<()>> {
        let clients = self.clients.clone();
        let addresses = self.addresses.clone();
        let sender = self.command_sender.clone();
        let initial_secs_delay = self.initial_secs_delay;
        let cycle_seconds = self.cycle_seconds;
        let max_cycles = self.max_cycles;
        let task = tokio::spawn(async move {
            warn!("Address subscriber task starting...");
            let mut cycle: usize = 0;
            let mut stopwatch = Instant::now();
            loop {
                if cycle == 0 {
                    tokio::select! {
                        biased;
                        _ = stop_signal.listener.clone() => {
                            break;
                        }
                        _ = sleep(stopwatch + Duration::from_secs(initial_secs_delay) - Instant::now()) => {}
                    }
                    stopwatch = Instant::now();
                }
                cycle += 1;

                if cycle <= max_cycles {
                    warn!("Cycle {cycle} - Starting UTXOs notifications...");
                    let (tx, rx) = channel();
                    sender.send(SubscribeCommand::RegisterJob((clients.len(), tx))).await.unwrap();
                    let registration = rx.await.unwrap();
                    for (i, client) in clients.iter().cloned().enumerate() {
                        sender
                            .send(SubscribeCommand::StartUtxosChanged((registration.id, client, addresses[i].clone())))
                            .await
                            .unwrap();
                    }
                    tokio::select! {
                        biased;
                        _ = stop_signal.listener.clone() => {
                            break;
                        }
                        _ = registration.complete => {}
                    }
                    warn!("Cycle {cycle} - UTXOs notifications started");
                }

                tokio::select! {
                    biased;
                    _ = stop_signal.listener.clone() => {
                        break;
                    }
                    _ = sleep(stopwatch + Duration::from_secs(cycle_seconds - (cycle_seconds / 3)) - Instant::now()) => {}
                }
                stopwatch = Instant::now();

                if cycle < max_cycles {
                    warn!("Cycle {cycle} - Stopping UTXOs notifications...");
                    let (tx, rx) = channel();
                    sender.send(SubscribeCommand::RegisterJob((clients.len(), tx))).await.unwrap();
                    let registration = rx.await.unwrap();
                    for client in clients.iter().cloned() {
                        sender.send(SubscribeCommand::StopUtxosChanged((registration.id, client))).await.unwrap();
                    }
                    tokio::select! {
                        biased;
                        _ = stop_signal.listener.clone() => {
                            break;
                        }
                        _ = registration.complete => {}
                    }
                    warn!("Cycle {cycle} - UTXOs notifications stopped");
                }

                tokio::select! {
                    biased;
                    _ = stop_signal.listener.clone() => {
                        break;
                    }
                    _ = sleep(stopwatch + Duration::from_secs(cycle_seconds / 3) - Instant::now()) => {}
                }
                stopwatch = Instant::now();
            }
            for client in clients.iter() {
                client.disconnect().await.unwrap();
            }
            warn!("Address subscriber task exited");
        });
        vec![task]
    }
}
