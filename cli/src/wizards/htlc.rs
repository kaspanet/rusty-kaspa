use crate::KaspaCli;
use kaspa_wallet_core::runtime::Account;
use kaspa_wallet_core::{
    runtime::{wallet::HtlcCreateArgs, PrvKeyDataCreateArgs},
    storage::account::HtlcRole,
};
use std::sync::Arc;

pub(crate) async fn create(
    ctx: &Arc<KaspaCli>,
    name: Option<&str>,
    role: HtlcRole,
    locktime: u64,
    secret_hash: String,
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
    term.writeln("Mnemonic Phrase: ");
    term.writeln(mnemonic.phrase());
    let second_party_address = term.ask(false, "Enter second party address: ").await?;
    wallet
        .create_htlc_account(HtlcCreateArgs {
            prv_key_data_id,
            name,
            title,
            wallet_secret,
            second_party_address,
            role,
            locktime,
            secret_hash,
        })
        .await?;
    Ok(())
}

pub(crate) async fn add_secret(ctx: &Arc<KaspaCli>, acc: Arc<dyn Account>, secret: String) -> crate::imports::Result<()> {
    let wallet = ctx.wallet();
    let (wallet_secret, _) = ctx.ask_wallet_secret(None).await?;
    let mut secret_v = vec![0; secret.len() / 2];
    faster_hex::hex_decode(secret.as_bytes(), &mut secret_v)?;
    wallet.htlc_add_secret(acc, wallet_secret, secret_v).await?;
    Ok(())
}

pub(crate) async fn show(ctx: &Arc<KaspaCli>, acc: Arc<dyn Account>) -> crate::imports::Result<kaspa_addresses::Address> {
    let wallet = ctx.wallet();
    let addr = wallet.htlc_show_address(acc).await?;
    Ok(addr)
}
