use crate::cli::KaspaCli;
use crate::imports::*;
use crate::result::Result;
use kaspa_wallet_core::storage::AccountKind;

pub(crate) async fn create(
    ctx: &Arc<KaspaCli>,
    prv_key_data_id: PrvKeyDataId,
    account_kind: AccountKind,
    name: Option<&str>,
) -> Result<()> {
    let term = ctx.term();
    let wallet = ctx.wallet();

    if matches!(account_kind, AccountKind::MultiSig) {
        return Err(Error::Custom(
            "MultiSig accounts are not currently supported (will be available in the future version)".to_string(),
        ));
    }

    let (title, name) = if let Some(name) = name {
        (name.to_string(), name.to_string())
    } else {
        let title = term.ask(false, "Please enter account title (optional, press <enter> to skip): ").await?.trim().to_string();
        let name = title.replace(' ', "-").to_lowercase();
        (title, name)
    };

    let wallet_secret = Secret::new(term.ask(true, "Enter wallet password: ").await?.trim().as_bytes().to_vec());
    if wallet_secret.as_ref().is_empty() {
        return Err(Error::WalletSecretRequired);
    }

    let prv_key_info = wallet.store().as_prv_key_data_store()?.load_key_info(&prv_key_data_id).await?;
    if let Some(keyinfo) = prv_key_info {
        let payment_secret = if keyinfo.is_encrypted() {
            let payment_secret = Secret::new(term.ask(true, "Enter payment password: ").await?.trim().as_bytes().to_vec());
            if payment_secret.as_ref().is_empty() {
                return Err(Error::PaymentSecretRequired);
            } else {
                Some(payment_secret)
            }
        } else {
            None
        };

        let account_args = AccountCreateArgs::new(name, title, account_kind, wallet_secret, payment_secret);
        let account = wallet.create_bip32_account(prv_key_data_id, account_args).await?;

        tprintln!(ctx, "\naccount created: {}\n", account.get_list_string()?);
        wallet.select(Some(&account)).await?;
    } else {
        return Err(Error::KeyDataNotFound);
    }

    Ok(())
}
