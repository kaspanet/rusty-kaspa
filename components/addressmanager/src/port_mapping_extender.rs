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

pub const SERVICE_NAME: &str = "port-mapping-extender";

pub struct Extender {
    tick_service: Arc<TickService>,
    fetch_interval: Duration,
    deadline_sec: u64,
    gateway: igd_next::aio::Gateway<Tokio>,
    external_port: u16,
    local_addr: SocketAddr,
}

impl Extender {
    pub fn new(
        tick_service: Arc<TickService>,
        fetch_interval: Duration,
        deadline_sec: u64,
        gateway: igd_next::aio::Gateway<Tokio>,
        external_port: u16,
        local_addr: SocketAddr,
    ) -> Self {
        Self { tick_service, fetch_interval, deadline_sec, gateway, external_port, local_addr }
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
        }
        // Let the system print final logs before exiting
        tokio::time::sleep(Duration::from_millis(500)).await;
        trace!("{SERVICE_NAME} worker exiting");
        Ok(())
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
