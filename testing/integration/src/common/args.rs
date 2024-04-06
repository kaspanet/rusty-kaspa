use crate::tasks::daemon::DaemonArgs;
#[cfg(feature = "devnet-prealloc")]
use kaspa_addresses::Address;
use kaspad_lib::args::Args;

pub struct ArgsBuilder {
    args: Args,
}

impl ArgsBuilder {
    #[cfg(feature = "devnet-prealloc")]
    pub fn simnet(num_prealloc_utxos: u64, prealloc_amount: u64) -> Self {
        let args = Args {
            simnet: true,
            disable_upnp: true, // UPnP registration might take some time and is not needed for this test
            enable_unsynced_mining: true,
            num_prealloc_utxos: Some(num_prealloc_utxos),
            prealloc_amount: prealloc_amount * kaspa_consensus_core::constants::SOMPI_PER_KASPA,
            block_template_cache_lifetime: Some(0),
            rpc_max_clients: 2500,
            unsafe_rpc: true,
            ..Default::default()
        };

        Self { args }
    }

    #[cfg(not(feature = "devnet-prealloc"))]
    pub fn simnet() -> Self {
        let args = Args {
            simnet: true,
            disable_upnp: true, // UPnP registration might take some time and is not needed for this test
            enable_unsynced_mining: true,
            block_template_cache_lifetime: Some(0),
            rpc_max_clients: 2500,
            unsafe_rpc: true,
            ..Default::default()
        };

        Self { args }
    }

    #[cfg(feature = "devnet-prealloc")]
    pub fn prealloc_address(mut self, prealloc_address: Address) -> Self {
        self.args.prealloc_address = Some(prealloc_address.to_string());
        self
    }

    pub fn rpc_max_clients(mut self, rpc_max_clients: usize) -> Self {
        self.args.rpc_max_clients = rpc_max_clients;
        self
    }

    pub fn max_tracked_addresses(mut self, max_tracked_addresses: usize) -> Self {
        self.args.max_tracked_addresses = max_tracked_addresses;
        self
    }

    pub fn utxoindex(mut self, utxoindex: bool) -> Self {
        self.args.utxoindex = utxoindex;
        self
    }

    pub fn apply_args<F>(mut self, edit_func: F) -> Self
    where
        F: Fn(&mut Args),
    {
        edit_func(&mut self.args);
        self
    }

    pub fn apply_daemon_args(mut self, daemon_args: &DaemonArgs) -> Self {
        daemon_args.apply_to(&mut self.args);
        self
    }

    pub fn build(self) -> Args {
        self.args
    }
}
