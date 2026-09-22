use std::sync::Arc;

use kaspa_core::signals::Shutdown;
use kaspa_utils::fd_budget;
use kaspad_lib::{args as kaspad_args, daemon as kaspad_daemon};

pub(crate) struct InProcessNode {
    core: Arc<kaspa_core::core::Core>,
    workers: Vec<std::thread::JoinHandle<()>>,
}

impl InProcessNode {
    pub(crate) fn start_from_args(args: kaspad_args::Args) -> Result<Self, anyhow::Error> {
        let _ = fd_budget::try_set_fd_limit(kaspad_daemon::DESIRED_DAEMON_SOFT_FD_LIMIT);

        let runtime = kaspad_daemon::Runtime::from_args(&args);
        let rpc_max_clients = i32::try_from(args.rpc_max_clients)?;
        let inbound_limit = i32::try_from(args.inbound_limit)?;
        let outbound_target = i32::try_from(args.outbound_target)?;
        let fd_total_budget = fd_budget::limit()
            .checked_sub(rpc_max_clients)
            .and_then(|budget| budget.checked_sub(inbound_limit))
            .and_then(|budget| budget.checked_sub(outbound_target))
            .ok_or_else(|| anyhow::anyhow!("file descriptor budget subtraction overflow"))?;
        anyhow::ensure!(fd_total_budget > 0, "client and peer limits exhaust the file descriptor budget");
        let (core, _) = kaspad_daemon::create_core_with_runtime(&runtime, &args, fd_total_budget);
        let workers = core.start();
        Ok(Self { core, workers })
    }

    fn shutdown(self) {
        self.core.shutdown();
        self.core.join(self.workers);
    }
}

pub(crate) async fn shutdown_inprocess(node: InProcessNode) {
    let _ = tokio::task::spawn_blocking(move || node.shutdown()).await;
}
