pub mod data;
pub mod error;
pub mod result;

pub use data::{Metric, MetricGroup, MetricsData, MetricsSnapshot};

use crate::result::Result;
use futures::{future::join_all, pin_mut, select, FutureExt, StreamExt};
use kaspa_rpc_core::{api::rpc::RpcApi, GetMetricsResponse, RpcPeerInfo};
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

pub type MetricsSinkFn =
    Arc<Box<dyn Send + Sync + Fn(MetricsSnapshot) -> Option<Pin<Box<(dyn Send + 'static + Future<Output = Result<()>>)>>> + 'static>>;

pub struct Metrics {
    task_ctl: DuplexChannel,
    rpc: Arc<Mutex<Option<Arc<dyn RpcApi>>>>,
    sink: Arc<Mutex<Option<MetricsSinkFn>>>,
    data: Arc<Mutex<Option<MetricsData>>>,
    connected_peer_info: Arc<Mutex<Option<Arc<Vec<RpcPeerInfo>>>>>,
}

impl Default for Metrics {
    fn default() -> Self {
        Metrics {
            task_ctl: DuplexChannel::oneshot(),
            rpc: Arc::new(Mutex::new(None)),
            sink: Arc::new(Mutex::new(None)),
            data: Arc::new(Mutex::new(None)),
            connected_peer_info: Arc::new(Mutex::new(None)),
        }
    }
}

impl Metrics {
    pub fn set_rpc(&self, rpc: Option<Arc<dyn RpcApi>>) {
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

    pub fn connected_peer_info(&self) -> Option<Arc<Vec<RpcPeerInfo>>> {
        self.connected_peer_info.lock().unwrap().clone()
    }

    pub async fn start_task(self: &Arc<Self>) -> Result<()> {
        let this = self.clone();

        let task_ctl_receiver = self.task_ctl.request.receiver.clone();
        let task_ctl_sender = self.task_ctl.response.sender.clone();

        *this.data.lock().unwrap() = Some(MetricsData::new(unixtime_as_millis_f64()));

        spawn(async move {
            let interval = interval(Duration::from_secs(1));
            pin_mut!(interval);

            loop {
                select! {
                    _ = task_ctl_receiver.recv().fuse() => {
                        break;
                    },
                    _ = interval.next().fuse() => {

                            let last_data = this.data.lock().unwrap().take().unwrap();
                            this.data.lock().unwrap().replace(MetricsData::new(unixtime_as_millis_f64()));
                            if let Some(rpc) = this.rpc() {
                                let samples = vec![
                                    this.sample_metrics(rpc.clone()).boxed(),
                                    this.sample_get_block_dag_info(rpc.clone()).boxed(),
                                    this.sample_connected_peer_info(rpc.clone()).boxed(),
                                ];

                                join_all(samples).await;
                            } else {
                                this.connected_peer_info.lock().unwrap().take();
                            }

                            if let Some(sink) = this.sink() {
                                let snapshot = MetricsSnapshot::from((&last_data, this.data.lock().unwrap().as_ref().unwrap()));
                                if let Some(future) = sink(snapshot) {
                                    future.await.ok();
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

    // --- samplers

    async fn sample_metrics(self: &Arc<Self>, rpc: Arc<dyn RpcApi>) -> Result<()> {
        if let Ok(metrics) = rpc.get_metrics(true, true, true).await {
            let GetMetricsResponse { server_time: _, consensus_metrics, connection_metrics: _, process_metrics } = metrics;

            let mut data = self.data.lock().unwrap();
            let data = data.as_mut().unwrap();
            if let Some(consensus_metrics) = consensus_metrics {
                data.blocks_submitted = consensus_metrics.blocks_submitted;
                data.header_counts = consensus_metrics.header_counts;
                data.dep_counts = consensus_metrics.dep_counts;
                data.body_counts = consensus_metrics.body_counts;
                data.txs_counts = consensus_metrics.txs_counts;
                data.chain_block_counts = consensus_metrics.chain_block_counts;
                data.mass_counts = consensus_metrics.mass_counts;
            }

            if let Some(process_metrics) = process_metrics {
                data.resident_set_size_bytes = process_metrics.resident_set_size;
                data.virtual_memory_size_bytes = process_metrics.virtual_memory_size;
                data.cpu_cores = process_metrics.core_num;
                data.cpu_usage = process_metrics.cpu_usage;
                data.fd_num = process_metrics.fd_num;
                data.disk_io_read_bytes = process_metrics.disk_io_read_bytes;
                data.disk_io_write_bytes = process_metrics.disk_io_write_bytes;
                data.disk_io_read_per_sec = process_metrics.disk_io_read_per_sec;
                data.disk_io_write_per_sec = process_metrics.disk_io_write_per_sec;
            }
        }

        Ok(())
    }

    async fn sample_get_block_dag_info(self: &Arc<Self>, rpc: Arc<dyn RpcApi>) -> Result<()> {
        if let Ok(gdbi) = rpc.get_block_dag_info().await {
            let mut data = self.data.lock().unwrap();
            let data = data.as_mut().unwrap();
            data.block_count = gdbi.block_count;
            // data.header_count = gdbi.header_count;
            data.tip_hashes = gdbi.tip_hashes.len();
            data.difficulty = gdbi.difficulty;
            data.past_median_time = gdbi.past_median_time;
            data.virtual_parent_hashes = gdbi.virtual_parent_hashes.len();
            data.virtual_daa_score = gdbi.virtual_daa_score;
        }

        Ok(())
    }

    async fn sample_connected_peer_info(self: &Arc<Self>, rpc: Arc<dyn RpcApi>) -> Result<()> {
        if let Ok(cpi) = rpc.get_connected_peer_info().await {
            self.connected_peer_info.lock().unwrap().replace(Arc::new(cpi.peer_info));
        }

        Ok(())
    }
}