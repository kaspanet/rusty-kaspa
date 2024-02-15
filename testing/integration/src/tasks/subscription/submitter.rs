use crate::tasks::Task;
use async_channel::Sender;
use async_trait::async_trait;
use itertools::Itertools;
use kaspa_addresses::Address;
use kaspa_core::warn;
use kaspa_grpc_client::GrpcClient;
use kaspa_notify::scope::{Scope, UtxosChangedScope};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_utils::{channel::Channel, triggers::SingleTrigger};
use parking_lot::Mutex;
use rand::thread_rng;
use rand_distr::{Distribution, Exp};
use std::{cmp::max, collections::HashMap, sync::Arc, time::Duration};
use tokio::{
    sync::oneshot::{channel as oneshot_channel, Receiver as OneshotReceiver, Sender as OneshotSender},
    task::JoinHandle,
    time::sleep,
};

pub type JobId = u64;
pub type Count = usize;

pub struct Registration {
    pub id: JobId,
    pub complete: OneshotReceiver<()>,
}

impl Registration {
    pub fn new(id: JobId, complete: OneshotReceiver<()>) -> Self {
        Self { id, complete }
    }
}

pub enum SubscribeCommand {
    RegisterJob((Count, OneshotSender<Registration>)),
    Start((JobId, Arc<GrpcClient>, Scope)),
    Stop((JobId, Arc<GrpcClient>, Scope)),
    StartUtxosChanged((JobId, Arc<GrpcClient>, Arc<Vec<Address>>)),
    StopUtxosChanged((JobId, Arc<GrpcClient>)),
}

struct Job {
    count: Count,
    feedback: OneshotSender<()>,
}

impl Job {
    fn new(count: Count, feedback: OneshotSender<()>) -> Self {
        Self { count, feedback }
    }
}

#[derive(Default)]
struct JobRegister {
    jobs: HashMap<JobId, Job>,
    max_id: JobId,
}

impl JobRegister {
    fn register(&mut self, count: Count) -> Registration {
        self.max_id += 1;
        let id = self.max_id;
        let (feedback, complete) = oneshot_channel();
        let job = Job::new(count, feedback);
        self.jobs.insert(id, job);
        Registration::new(id, complete)
    }

    fn dec_count(&mut self, id: JobId) {
        let job = self.jobs.get_mut(&id).unwrap();
        job.count -= 1;
        if job.count > 0 {
            return;
        }
        let (_, job) = self.jobs.remove_entry(&id).unwrap();
        let _ = job.feedback.send(());
    }
}

pub struct SubscriptionSubmitterTask {
    workers: usize,
    distribution_channel: Channel<SubscribeCommand>,
    bps: u64,
    register: Arc<Mutex<JobRegister>>,
}

impl SubscriptionSubmitterTask {
    pub fn new(workers: usize, distribution_channel_capacity: usize, bps: u64) -> Self {
        let distribution_channel = Channel::bounded(distribution_channel_capacity);
        let register = Default::default();
        Self { workers, distribution_channel, bps, register }
    }

    pub fn build(workers: usize, distribution_channel_capacity: usize, bps: u64) -> Arc<Self> {
        Arc::new(Self::new(workers, distribution_channel_capacity, bps))
    }

    pub fn sender(&self) -> Sender<SubscribeCommand> {
        self.distribution_channel.sender()
    }

    pub fn close(&self) {
        self.distribution_channel.close()
    }
}

#[async_trait]
impl Task for SubscriptionSubmitterTask {
    fn start(&self, stop_signal: SingleTrigger) -> Vec<JoinHandle<()>> {
        warn!("Subscription submitter task starting...");
        let distribution: Exp<f64> = Exp::new(self.bps as f64).unwrap();
        let mut tasks = (0..self.workers)
            .map(|_| {
                let rx = self.distribution_channel.receiver();
                let dist = distribution;
                let register = self.register.clone();
                tokio::spawn(async move {
                    while let Ok(command) = rx.recv().await {
                        match command {
                            SubscribeCommand::RegisterJob((count, sender)) => {
                                assert!(count > 0);
                                let registration = register.lock().register(count);
                                let _ = sender.send(registration);
                            }
                            SubscribeCommand::Start((id, client, scope)) => {
                                client.start_notify(0, scope).await.unwrap();
                                register.lock().dec_count(id);
                            }
                            SubscribeCommand::Stop((id, client, scope)) => {
                                client.stop_notify(0, scope).await.unwrap();
                                register.lock().dec_count(id);
                            }
                            SubscribeCommand::StartUtxosChanged((id, client, addresses)) => loop {
                                match client.start_notify(0, UtxosChangedScope::new((*addresses).clone()).into()).await {
                                    Ok(_) => {
                                        register.lock().dec_count(id);
                                        break;
                                    }
                                    Err(err) => {
                                        warn!("Failed to start a subscription with {} addresses: {}", addresses.len(), err);
                                        let timeout = max((dist.sample(&mut thread_rng()) * 200.0) as u64, 1);
                                        sleep(Duration::from_millis(timeout)).await;
                                    }
                                }
                            },
                            SubscribeCommand::StopUtxosChanged((id, client)) => loop {
                                match client.stop_notify(0, UtxosChangedScope::new(vec![]).into()).await {
                                    Ok(_) => {
                                        register.lock().dec_count(id);
                                        break;
                                    }
                                    Err(err) => {
                                        warn!("Failed to stop a subscription: {}", err);
                                        let timeout = max((dist.sample(&mut thread_rng()) * 250.0) as u64, 1);
                                        sleep(Duration::from_millis(timeout)).await;
                                    }
                                }
                            },
                        }
                    }
                })
            })
            .collect_vec();

        let sender = self.sender();
        let shutdown_task = tokio::spawn(async move {
            stop_signal.listener.await;
            let _ = sender.close();
            warn!("Subscription submitter task exited");
        });
        tasks.push(shutdown_task);

        tasks
    }
}
