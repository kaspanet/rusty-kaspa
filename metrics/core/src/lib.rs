pub mod data;
pub mod error;
pub mod result;

pub use data::{Metric, MetricGroup, MetricsData, MetricsSnapshot};

use crate::result::Result;
use futures::{pin_mut, select, FutureExt, StreamExt};
use kaspa_rpc_core::api::rpc::RpcApi;
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};
use workflow_core::channel::DuplexChannel;
use workflow_core::task::interval;
use workflow_core::task::spawn;
use workflow_core::time::unixtime_as_millis_f64;
use workflow_log::*;

pub type MetricsSinkFn =
    Arc<Box<dyn Send + Sync + Fn(MetricsSnapshot) -> Option<Pin<Box<dyn Send + 'static + Future<Output = Result<()>>>>> + 'static>>;

pub struct Metrics {
    task_ctl: DuplexChannel,
    rpc: Arc<Mutex<Option<Arc<dyn RpcApi>>>>,
    sink: Arc<Mutex<Option<MetricsSinkFn>>>,
    data: Arc<Mutex<Option<MetricsData>>>,
}

impl Default for Metrics {
    fn default() -> Self {
        Metrics {
            task_ctl: DuplexChannel::oneshot(),
            rpc: Arc::new(Mutex::new(None)),
            sink: Arc::new(Mutex::new(None)),
            data: Arc::new(Mutex::new(None)),
        }
    }
}

impl Metrics {
    pub fn bind_rpc(&self, rpc: Option<Arc<dyn RpcApi>>) {
        *self.rpc.lock().unwrap() = rpc;
    }

    fn rpc(&self) -> Option<Arc<dyn RpcApi>> {
        self.rpc.lock().unwrap().clone()
    }

    pub fn register_sink(&self, target: MetricsSinkFn) {
        self.sink.lock().unwrap().replace(target);
    }

    pub fn unregister_sink(&self) {
        self.sink.lock().unwrap().take();
    }

    pub fn sink(&self) -> Option<MetricsSinkFn> {
        self.sink.lock().unwrap().clone()
    }

    pub async fn start_task(self: &Arc<Self>) -> Result<()> {
        let this = self.clone();

        let task_ctl_receiver = self.task_ctl.request.receiver.clone();
        let task_ctl_sender = self.task_ctl.response.sender.clone();

        let mut current_metrics_data = MetricsData::new(unixtime_as_millis_f64());
        *this.data.lock().unwrap() = Some(current_metrics_data.clone());

        spawn(async move {
            let interval = interval(Duration::from_secs(1));
            pin_mut!(interval);

            let mut first = true;

            loop {
                select! {
                    _ = task_ctl_receiver.recv().fuse() => {
                        break;
                    },
                    _ = interval.next().fuse() => {

                        if let Some(rpc) = this.rpc() {
                                match this.sample_metrics(rpc.clone()).await {
                                    Ok(incoming_data) => {
                                    let last_metrics_data = current_metrics_data;
                                    current_metrics_data = incoming_data;
                                    this.data.lock().unwrap().replace(current_metrics_data.clone());

                                    if first {
                                        first = false;
                                    } else if let Some(sink) = this.sink() {
                                        let snapshot = MetricsSnapshot::from((&last_metrics_data, &current_metrics_data));
                                        if let Some(future) = sink(snapshot) {
                                            future.await.ok();
                                        }
                                    }

                                }
                                Err(err) => {
                                    log_trace!("Metrics::sample_metrics() error: {}", err);
                                }
                            }
                        }

                    }
                }
            }

            task_ctl_sender.send(()).await.unwrap();
        });
        Ok(())
    }

    pub async fn stop_task(&self) -> Result<()> {
        self.task_ctl.signal(()).await.expect("Metrics::stop_task() signal error");
        Ok(())
    }

    async fn sample_metrics(self: &Arc<Self>, rpc: Arc<dyn RpcApi>) -> Result<MetricsData> {
        MetricsData::try_from(rpc.get_metrics(true, true, true, true, true, false).await?)
    }
}
