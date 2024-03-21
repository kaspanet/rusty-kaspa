use crate::{
    common::client_notify::ChannelNotify,
    tasks::{Stopper, Task},
};
use async_trait::async_trait;
use kaspa_addresses::Address;
use kaspa_core::warn;
use kaspa_grpc_client::GrpcClient;
use kaspa_notify::{listener::ListenerId, scope::NewBlockTemplateScope};
use kaspa_rpc_core::{api::rpc::RpcApi, GetBlockTemplateResponse, Notification};
use kaspa_utils::{channel::Channel, triggers::SingleTrigger};
use parking_lot::Mutex;
use std::sync::Arc;
use tokio::task::JoinHandle;

pub struct BlockTemplateReceiverTask {
    client: Arc<GrpcClient>,
    channel: Channel<Notification>,
    template: Arc<Mutex<GetBlockTemplateResponse>>,
    pay_address: Address,
    stopper: Stopper,
}

impl BlockTemplateReceiverTask {
    pub fn new(
        client: Arc<GrpcClient>,
        channel: Channel<Notification>,
        response: GetBlockTemplateResponse,
        pay_address: Address,
        stopper: Stopper,
    ) -> Self {
        let template = Arc::new(Mutex::new(response));
        Self { client, channel, template, pay_address, stopper }
    }

    pub async fn build(client: Arc<GrpcClient>, pay_address: Address, stopper: Stopper) -> Arc<Self> {
        let channel = Channel::default();
        client.start(Some(Arc::new(ChannelNotify::new(channel.sender())))).await;
        client.start_notify(ListenerId::default(), NewBlockTemplateScope {}.into()).await.unwrap();
        let response = client.get_block_template(pay_address.clone(), vec![]).await.unwrap();
        Arc::new(Self::new(client, channel, response, pay_address, stopper))
    }

    pub fn template(&self) -> Arc<Mutex<GetBlockTemplateResponse>> {
        self.template.clone()
    }
}

#[async_trait]
impl Task for BlockTemplateReceiverTask {
    fn start(&self, stop_signal: SingleTrigger) -> Vec<JoinHandle<()>> {
        let client = self.client.clone();
        let receiver = self.channel.receiver();
        let pay_address = self.pay_address.clone();
        let template = self.template();
        let stopper = self.stopper;
        let task = tokio::spawn(async move {
            warn!("Block template receiver task starting...");
            loop {
                tokio::select! {
                    biased;
                    _ = stop_signal.listener.clone() => {
                        break;
                    }
                    result = receiver.recv() => {
                        match result {
                            Ok(notification) => {
                                match notification {
                                    Notification::NewBlockTemplate(_) => {
                                        // Drain the channel
                                        while receiver.try_recv().is_ok() {}
                                        let response = client.get_block_template(pay_address.clone(), vec![]).await.unwrap();
                                        *template.lock() = response;
                                    }
                                    _ => panic!(),
                                }
                            }
                            Err(_) => {
                                break;
                            }
                        }
                    }
                }
            }
            if stopper == Stopper::Signal {
                stop_signal.trigger.trigger();
            }
            client.stop_notify(ListenerId::default(), NewBlockTemplateScope {}.into()).await.unwrap();
            client.disconnect().await.unwrap();
            warn!("Block template receiver task exited");
        });
        vec![task]
    }
}
