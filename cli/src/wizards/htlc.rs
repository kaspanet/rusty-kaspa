use crate::KaspaCli;
use kaspa_wallet_core::runtime::wallet::Role;
use kaspa_wallet_core::runtime::{AccountKind, PrvKeyDataCreateArgs};
use kaspa_wallet_core::storage::PrvKeyDataInfo;
use std::sync::Arc;

pub(crate) async fn create(
    ctx: &Arc<KaspaCli>,
    prv_key_data_info: Arc<PrvKeyDataInfo>,
    name: Option<&str>,
    role: Role,
) -> crate::imports::Result<()> {
    let term = ctx.term();
    let wallet = ctx.wallet();
    let (wallet_secret, _) = ctx.ask_wallet_secret(None).await?;

    let (title, name) = if let Some(name) = name {
        (Some(name.to_string()), Some(name.to_string()))
    } else {
        let title = term.ask(false, "Please enter account title (optional, press <enter> to skip): ").await?.trim().to_string();
        let name = title.replace(' ', "-").to_lowercase();
        (Some(title), Some(name))
    };

    let prv_key_data_args = PrvKeyDataCreateArgs::new(None, wallet_secret.clone(), None);
    let (prv_key_data_id, mnemonic) = wallet.create_prv_key_data(prv_key_data_args).await?;
    let second_party_xpub_key = term.ask(false, &format!("Enter extended public {i} key: ")).await?;

    wallet.create_htlc_account()
}
