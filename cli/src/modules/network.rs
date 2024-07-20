use crate::imports::*;

#[derive(Default, Handler)]
#[help("Select network type (mainnet|testnet)")]
pub struct Network;

impl Network {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;

        if let Some(network_id) = argv.first() {
            let network_id: NetworkId = network_id.trim().parse::<NetworkId>()?;
            tprintln!(ctx, "Setting network id to: {network_id}");
            ctx.wallet().set_network_id(&network_id)?;
            ctx.wallet().settings().set(WalletSettings::Network, network_id).await?;
        } else {
            let network_id = ctx.wallet().network_id()?;
            tprintln!(ctx, "Current network id is: {network_id}");
        }

        Ok(())
    }
}
