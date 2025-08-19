use igd_next::{aio::tokio::Tokio, AddPortError};
use kaspa_core::{
    debug, error, info,
    task::{
        service::{AsyncService, AsyncServiceFuture},
        tick::{TickReason, TickService},
    },
    trace, warn,
};
use std::{net::SocketAddr, sync::Arc, time::Duration};

use crate::UPNP_REGISTRATION_NAME;
use crate::{AddressManager, NetAddress};
use kaspa_utils::networking::IpAddress;
use parking_lot::Mutex;

pub const SERVICE_NAME: &str = "port-mapping-extender";

#[derive(Clone)]
pub struct Extender {
    tick_service: Arc<TickService>,
    fetch_interval: Duration,
    deadline_sec: u64,
    gateway: igd_next::aio::Gateway<Tokio>,
    external_port: u16,
    local_addr: SocketAddr,
    address_manager: Arc<Mutex<AddressManager>>,
    last_known_external_ip: Arc<Mutex<Option<std::net::IpAddr>>>,
}

impl Extender {
    pub fn new(
        tick_service: Arc<TickService>,
        fetch_interval: Duration,
        deadline_sec: u64,
        gateway: igd_next::aio::Gateway<Tokio>,
        external_port: u16,
        local_addr: SocketAddr,
        address_manager: Arc<Mutex<AddressManager>>,
        initial_external_ip: Option<std::net::IpAddr>,
    ) -> Self {
        // Log the initial IP for debugging
        if let Some(initial_ip) = initial_external_ip {
            debug!("[UPnP] Extender initialized with initial external IP: {}", initial_ip);
        }
        
        Self { 
            tick_service, 
            fetch_interval, 
            deadline_sec, 
            gateway, 
            external_port, 
            local_addr, 
            address_manager,
            last_known_external_ip: Arc::new(Mutex::new(initial_external_ip)),
        }
    }
}

impl Extender {
    pub async fn worker(&self) -> Result<(), AddPortError> {
        while let TickReason::Wakeup = self.tick_service.tick(self.fetch_interval).await {

            if let Err(e) = self
                .gateway
                .add_port(
                    igd_next::PortMappingProtocol::TCP,
                    self.external_port,
                    self.local_addr,
                    self.deadline_sec as u32,
                    UPNP_REGISTRATION_NAME,
                )
                .await
            {
                warn!("[UPnP] Extend external ip mapping err: {e:?}");
            } else {
                debug!("[UPnP] Extend external ip mapping");
            }

            let external_ip_result = self.gateway.get_external_ip().await;
            if let Err(e) = &external_ip_result {
                warn!("[UPnP] Fetch external ip err: {e:?}");
            } else {
                debug!("[UPnP] Fetched external ip");
            }
            
            if let Ok(current_ip) = external_ip_result {
                // Check if IP has changed
                let ip_changed = {
                    let mut last_ip_guard = self.last_known_external_ip.lock();
                    if *last_ip_guard != Some(current_ip) {
                        let old_ip = *last_ip_guard;
                        *last_ip_guard = Some(current_ip);
                        Some((current_ip, old_ip))
                    } else {
                        None
                    }
                }; // MutexGuard is dropped here
                
                if let Some((new_ip, old_ip)) = ip_changed {
                    self.handle_ip_change(new_ip, old_ip).await;
                }
            }

            
        }
        // Let the system print final logs before exiting
        tokio::time::sleep(Duration::from_millis(500)).await;
        trace!("{SERVICE_NAME} worker exiting");
        Ok(())
    }

    async fn handle_ip_change(&self, new_ip: std::net::IpAddr, old_ip: Option<std::net::IpAddr>) {
        info!("[UPnP] External IP changed from {:?} to {}", old_ip, new_ip);
        
        // Update best_local_address
        let mut am_guard = self.address_manager.lock();
        let ip = IpAddress::new(new_ip);
        let net_addr = NetAddress { ip, port: self.external_port };
        am_guard.set_best_local_address(net_addr);
        debug!("[UPnP] Updated best local address to {}", net_addr);
        
        // Notify registered sinks (fire-and-forget). We offload each sync callback into its
        // own lightweight task so the extender loop isn't delayed by sink logic.
        let sinks = am_guard.clone_external_ip_change_sinks();
        drop(am_guard);
        for sink in sinks {
            let s = sink.clone();
            tokio::spawn(async move {
                // Trait is sync; we just invoke inside an async task (fire-and-forget).
                s.on_external_ip_changed(new_ip, old_ip);
            });
        }
    }
}

impl AsyncService for Extender {
    fn ident(self: Arc<Self>) -> &'static str {
        SERVICE_NAME
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.worker().await.unwrap_or_else(|e| {
                error!("worker error: {e:?}");
            });
            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        trace!("sending an exit signal to {}", SERVICE_NAME);
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            if let Err(err) = self.gateway.remove_port(igd_next::PortMappingProtocol::TCP, self.external_port).await {
                warn!("[UPnP] Remove port mapping err: {err:?}");
            } else {
                info!("[UPnP] Successfully removed port mapping, external port: {}", self.external_port);
            }
            trace!("{} stopped", SERVICE_NAME);
            Ok(())
        })
    }
}
