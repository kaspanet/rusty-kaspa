use std::{net::IpAddr, sync::{Arc}, time::Duration};
use parking_lot::Mutex;
use kaspa_core::{info, debug, warn, trace, task::{tick::{TickService, TickReason}, service::{AsyncService, AsyncServiceFuture}}};
use crate::{NetAddress, AddressManager};
use kaspa_utils::networking::IpAddress;
use kaspa_consensus_core::config::{Config, IpVersionMode};
use std::net::ToSocketAddrs;

pub const SERVICE_NAME: &str = "dyndns-extender";

// Simplistic resolver trait to allow mocking in tests later
trait DynResolver: Send + Sync {
    fn resolve(&self, host: &str) -> std::io::Result<Vec<IpAddr>>;
}

struct DefaultResolver;
impl DynResolver for DefaultResolver {
    fn resolve(&self, host: &str) -> std::io::Result<Vec<IpAddr>> { Ok((host, 0).to_socket_addrs()?.map(|sa| sa.ip()).collect()) }
}

pub struct DynDnsExtender {
    tick_service: Arc<TickService>,
    address_manager: Arc<Mutex<AddressManager>>,
    host: String,
    min_refresh: Duration,
    max_refresh: Duration,
    ip_mode: IpVersionMode,
    resolver: Box<dyn DynResolver>,
    last_ip: Arc<Mutex<Option<IpAddr>>>,
}

impl DynDnsExtender {
    pub fn new(config: Arc<Config>, am: Arc<Mutex<AddressManager>>, tick_service: Arc<TickService>) -> Option<Self> {
        let host = config.external_dyndns_host.clone()?; // only build if host provided

        let instance = Self {
            tick_service,
            address_manager: am.clone(),
            min_refresh: Duration::from_secs(config.external_dyndns_min_refresh_sec),
            max_refresh: Duration::from_secs(config.external_dyndns_max_refresh_sec),
            ip_mode: config.external_dyndns_ip_version,
            host,
            resolver: Box::new(DefaultResolver),
            last_ip: Arc::new(Mutex::new(None)),
        };

        Some(instance)
    }

    fn pick_ip(&self, mut ips: Vec<IpAddr>) -> Option<IpAddr> {
        // filter publics
        ips.retain(|ip| IpAddress::new(*ip).is_publicly_routable());
        if ips.is_empty() { return None; }
        match self.ip_mode {
            IpVersionMode::Ipv4 => ips.into_iter().find(|ip| matches!(ip, IpAddr::V4(_))),
            IpVersionMode::Ipv6 => ips.into_iter().find(|ip| matches!(ip, IpAddr::V6(_))),
            IpVersionMode::Auto => {
                if let Some(v4) = ips.iter().cloned().find(|ip| matches!(ip, IpAddr::V4(_))) { return Some(v4); }
                ips.into_iter().next()
            }
        }
    }

    async fn worker(&self) {
        info!("[DynDNS] Starting dyndns resolver for host {}", self.host);
        let mut interval = self.min_refresh; // adaptive later
        loop {
            match self.tick_service.tick(interval).await {
                TickReason::Shutdown => break,
                TickReason::Wakeup => {}
            }
            match self.resolver.resolve(&self.host) {
                Ok(ips) => {
                    debug!("[DynDNS] Resolved {} -> {:?}", self.host, ips);
                    let picked = self.pick_ip(ips);
                    if let Some(new_ip) = picked {
                        let mut last_guard = self.last_ip.lock();
                        if Some(new_ip) != *last_guard {
                            let old_ip = *last_guard;
                            *last_guard = Some(new_ip);
                            drop(last_guard);
                            self.apply_new_ip(new_ip, old_ip);
                        }
                        interval = self.min_refresh; // reset
                    } else {
                        warn!("[DynDNS] No public IP obtained for {}", self.host);
                        interval = std::cmp::min(interval * 2, self.max_refresh);
                    }
                }
                Err(e) => {
                    warn!("[DynDNS] Resolve failed for {}: {e}", self.host);
                    interval = std::cmp::min(interval * 2, self.max_refresh);
                }
            }
        }
        trace!("{SERVICE_NAME} worker exiting");
    }

    fn apply_new_ip(&self, new_ip: IpAddr, old_ip: Option<IpAddr>) {
        info!("[DynDNS] External IP changed {:?} -> {}", old_ip, new_ip);
        let mut am = self.address_manager.lock();
        let port = am.best_local_address().map(|a| a.port).unwrap_or_else(|| am.config.default_p2p_port());
        let net = NetAddress::new(new_ip.into(), port);
        am.set_best_local_address(net);
        let sinks = am.clone_external_ip_change_sinks();
        drop(am);
        for sink in sinks { let s = sink.clone(); tokio::spawn(async move { s.on_external_ip_changed(new_ip, old_ip); }); }
    }
}

impl AsyncService for DynDnsExtender {
    fn ident(self: Arc<Self>) -> &'static str { SERVICE_NAME }
    fn start(self: Arc<Self>) -> AsyncServiceFuture { Box::pin(async move { self.worker().await; Ok(()) }) }
    fn signal_exit(self: Arc<Self>) { trace!("sending an exit signal to {}", SERVICE_NAME); }
    fn stop(self: Arc<Self>) -> AsyncServiceFuture { Box::pin(async move { trace!("{} stopped", SERVICE_NAME); Ok(()) }) }
}
