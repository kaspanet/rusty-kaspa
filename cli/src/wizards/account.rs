use crate::cli::KaspaCli;
use crate::imports::*;
use crate::result::Result;
use kaspa_wallet_core::runtime::wallet::MultisigCreateArgs;
use kaspa_wallet_core::runtime::PrvKeyDataCreateArgs;
use kaspa_wallet_core::storage::AccountKind;

pub(crate) async fn create(
    ctx: &Arc<KaspaCli>,
    prv_key_data_info: Arc<PrvKeyDataInfo>,
    account_kind: AccountKind,
    name: Option<&str>,
) -> Result<()> {
    let term = ctx.term();
    let wallet = ctx.wallet();

    let (title, name) = if let Some(name) = name {
        (Some(name.to_string()), Some(name.to_string()))
    } else {
        let title = term.ask(false, "Please enter account title (optional, press <enter> to skip): ").await?.trim().to_string();
        let name = title.replace(' ', "-").to_lowercase();
        (Some(title), Some(name))
    };

    if matches!(account_kind, AccountKind::MultiSig) {
        return create_multisig(ctx, title, name).await;
    }

    let wallet_secret = Secret::new(term.ask(true, "Enter wallet password: ").await?.trim().as_bytes().to_vec());
    if wallet_secret.as_ref().is_empty() {
        return Err(Error::WalletSecretRequired);
    }

    let payment_secret = if prv_key_data_info.is_encrypted() {
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
    let account = wallet.create_bip32_account(prv_key_data_info.id, account_args).await?;

    tprintln!(ctx, "\naccount created: {}\n", account.get_list_string()?);
    wallet.select(Some(&account)).await?;
    Ok(())
}

async fn create_multisig(ctx: &Arc<KaspaCli>, title: Option<String>, name: Option<String>) -> Result<()> {
    let term = ctx.term();
    let wallet = ctx.wallet();
    let (wallet_secret, _) = ctx.ask_wallet_secret(None).await?;
    let n_required: u16 = term.ask(false, "Enter the minimum number of signatures required: ").await?.parse()?;

    let prv_keys_len: usize = term.ask(false, "Enter the number of private keys to generate: ").await?.parse()?;

    let mut prv_key_data_ids = Vec::with_capacity(prv_keys_len);
    let mut mnemonics = Vec::with_capacity(prv_keys_len);
    for _ in 0..prv_keys_len {
        let prv_key_data_args = PrvKeyDataCreateArgs::new(None, wallet_secret.clone(), None); // can be optimized with Rc<WalletSecret>
        let (prv_key_data_id, mnemonic) = wallet.create_prv_key_data(prv_key_data_args).await?;

        prv_key_data_ids.push(prv_key_data_id);
        mnemonics.push(mnemonic);
    }

    let additional_xpub_keys_len: usize = term.ask(false, "Enter the number of additional extended public keys: ").await?.parse()?;
    let mut xpub_keys = Vec::with_capacity(additional_xpub_keys_len + prv_keys_len);
    for i in 1..=additional_xpub_keys_len {
        let xpub_key = term.ask(false, &format!("Enter extended public {i} key: ")).await?;
        xpub_keys.push(xpub_key.trim().to_owned());
    }
    let account = wallet
        .create_multisig_account(MultisigCreateArgs {
            prv_key_data_ids,
            name,
            title,
            wallet_secret,
            additional_xpub_keys: xpub_keys,
            minimum_signatures: n_required,
        })
        .await?;
    tprintln!(ctx, "\naccount created: {}\n", account.get_list_string()?);
    wallet.select(Some(&account)).await?;
    Ok(())
}
