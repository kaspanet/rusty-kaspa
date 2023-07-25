use crate::imports::*;

#[derive(Default, Handler)]
#[help("Select network type (mainnet|testnet)")]
pub struct Network;

impl Network {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;

        if let Some(network_id) = argv.first() {
            let network_id: NetworkId = network_id.trim().parse::<NetworkId>()?;
            tprintln!(ctx, "Setting network type to: {network_id}");
            ctx.wallet().select_network(network_id)?;
            ctx.wallet().settings().set(WalletSettings::Network, network_id).await?;
        } else {
            let network_type = ctx.wallet().network()?;
            tprintln!(ctx, "Current network type is: {network_type}");
        }

        Ok(())
    }
}
