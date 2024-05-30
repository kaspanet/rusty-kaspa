pub mod data;
pub mod error;
pub mod result;

pub use data::{Metric, MetricGroup, MetricsData, MetricsSnapshot};

use crate::result::Result;
use futures::{pin_mut, select, FutureExt, StreamExt};
use kaspa_rpc_core::{api::rpc::RpcApi, GetMetricsResponse};
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
    Arc<Box<dyn Send + Sync + Fn(MetricsSnapshot) -> Option<Pin<Box<(dyn Send + 'static + Future<Output = Result<()>>)>>> + 'static>>;

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

            loop {
                select! {
                    _ = task_ctl_receiver.recv().fuse() => {
                        break;
                    },
                    _ = interval.next().fuse() => {

                        let last_metrics_data = current_metrics_data;
                        current_metrics_data = MetricsData::new(unixtime_as_millis_f64());

                        if let Some(rpc) = this.rpc() {
                            if let Err(err) = this.sample_metrics(rpc.clone(), &mut current_metrics_data).await {
                                log_trace!("Metrics::sample_metrics() error: {}", err);
                            }
                        }

                        this.data.lock().unwrap().replace(current_metrics_data.clone());

                        if let Some(sink) = this.sink() {
                            let snapshot = MetricsSnapshot::from((&last_metrics_data, &current_metrics_data));
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

    async fn sample_metrics(self: &Arc<Self>, rpc: Arc<dyn RpcApi>, data: &mut MetricsData) -> Result<()> {
        let GetMetricsResponse { server_time: _, consensus_metrics, connection_metrics, bandwidth_metrics, process_metrics } =
            rpc.get_metrics(true, true, true, true).await?;

        if let Some(consensus_metrics) = consensus_metrics {
            data.node_blocks_submitted_count = consensus_metrics.node_blocks_submitted_count;
            data.node_headers_processed_count = consensus_metrics.node_headers_processed_count;
            data.node_dependencies_processed_count = consensus_metrics.node_dependencies_processed_count;
            data.node_bodies_processed_count = consensus_metrics.node_bodies_processed_count;
            data.node_transactions_processed_count = consensus_metrics.node_transactions_processed_count;
            data.node_chain_blocks_processed_count = consensus_metrics.node_chain_blocks_processed_count;
            data.node_mass_processed_count = consensus_metrics.node_mass_processed_count;
            // --
            data.node_database_blocks_count = consensus_metrics.node_database_blocks_count;
            data.node_database_headers_count = consensus_metrics.node_database_headers_count;
            data.network_mempool_size = consensus_metrics.network_mempool_size;
            data.network_tip_hashes_count = consensus_metrics.network_tip_hashes_count;
            data.network_difficulty = consensus_metrics.network_difficulty;
            data.network_past_median_time = consensus_metrics.network_past_median_time;
            data.network_virtual_parent_hashes_count = consensus_metrics.network_virtual_parent_hashes_count;
            data.network_virtual_daa_score = consensus_metrics.network_virtual_daa_score;
        }

        if let Some(connection_metrics) = connection_metrics {
            data.node_borsh_live_connections = connection_metrics.borsh_live_connections;
            data.node_borsh_connection_attempts = connection_metrics.borsh_connection_attempts;
            data.node_borsh_handshake_failures = connection_metrics.borsh_handshake_failures;
            data.node_json_live_connections = connection_metrics.json_live_connections;
            data.node_json_connection_attempts = connection_metrics.json_connection_attempts;
            data.node_json_handshake_failures = connection_metrics.json_handshake_failures;
            data.node_active_peers = connection_metrics.active_peers;
        }

        if let Some(bandwidth_metrics) = bandwidth_metrics {
            data.node_borsh_bytes_tx = bandwidth_metrics.borsh_bytes_tx;
            data.node_borsh_bytes_rx = bandwidth_metrics.borsh_bytes_rx;
            data.node_json_bytes_tx = bandwidth_metrics.json_bytes_tx;
            data.node_json_bytes_rx = bandwidth_metrics.json_bytes_rx;
            data.node_p2p_bytes_tx = bandwidth_metrics.p2p_bytes_tx;
            data.node_p2p_bytes_rx = bandwidth_metrics.p2p_bytes_rx;
            data.node_grpc_user_bytes_tx = bandwidth_metrics.grpc_bytes_tx;
            data.node_grpc_user_bytes_rx = bandwidth_metrics.grpc_bytes_rx;

            data.node_total_bytes_tx = bandwidth_metrics.borsh_bytes_tx
                + bandwidth_metrics.json_bytes_tx
                + bandwidth_metrics.p2p_bytes_tx
                + bandwidth_metrics.grpc_bytes_tx;

            data.node_total_bytes_rx = bandwidth_metrics.borsh_bytes_rx
                + bandwidth_metrics.json_bytes_rx
                + bandwidth_metrics.p2p_bytes_rx
                + bandwidth_metrics.grpc_bytes_rx;
        }

        if let Some(process_metrics) = process_metrics {
            data.node_resident_set_size_bytes = process_metrics.resident_set_size;
            data.node_virtual_memory_size_bytes = process_metrics.virtual_memory_size;
            data.node_cpu_cores = process_metrics.core_num;
            data.node_cpu_usage = process_metrics.cpu_usage;
            data.node_file_handles = process_metrics.fd_num;
            data.node_disk_io_read_bytes = process_metrics.disk_io_read_bytes;
            data.node_disk_io_write_bytes = process_metrics.disk_io_write_bytes;
            data.node_disk_io_read_per_sec = process_metrics.disk_io_read_per_sec;
            data.node_disk_io_write_per_sec = process_metrics.disk_io_write_per_sec;
        }

        Ok(())
    }
}
