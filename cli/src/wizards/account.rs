use crate::cli::KaspaCli;
use crate::imports::*;
use crate::result::Result;
use kaspa_bip32::{Language, Mnemonic, WordCount};
use kaspa_wallet_core::account::MULTISIG_ACCOUNT_KIND;
// use kaspa_wallet_core::runtime::wallet::AccountCreateArgsBip32;
// use kaspa_wallet_core::runtime::{PrvKeyDataArgs, PrvKeyDataCreateArgs};
// use kaspa_wallet_core::storage::AccountKind;

pub(crate) async fn create(
    ctx: &Arc<KaspaCli>,
    prv_key_data_info: Arc<PrvKeyDataInfo>,
    account_kind: AccountKind,
    name: Option<&str>,
) -> Result<()> {
    let term = ctx.term();
    let wallet = ctx.wallet();

    // TODO @aspect
    let word_count = WordCount::Words12;

    let name = if let Some(name) = name {
        Some(name.to_string())
    } else {
        Some(term.ask(false, "Please enter account name (optional, press <enter> to skip): ").await?.trim().to_string())
    };

    if account_kind == MULTISIG_ACCOUNT_KIND {
        return create_multisig(ctx, name, word_count).await;
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

    let account_create_args_bip32 = AccountCreateArgsBip32::new(name, None);
    let account =
        wallet.create_account_bip32(&wallet_secret, prv_key_data_info.id, payment_secret.as_ref(), account_create_args_bip32).await?;

    tprintln!(ctx, "\naccount created: {}\n", account.get_list_string()?);
    wallet.select(Some(&account)).await?;
    Ok(())
}

async fn create_multisig(ctx: &Arc<KaspaCli>, account_name: Option<String>, mnemonic_phrase_word_count: WordCount) -> Result<()> {
    let term = ctx.term();
    let wallet = ctx.wallet();
    let (wallet_secret, _) = ctx.ask_wallet_secret(None).await?;
    let minimum_signatures: u16 = term.ask(false, "Enter the minimum number of signatures required: ").await?.parse()?;

    let prv_keys_len: usize = term.ask(false, "Enter the number of private keys to generate: ").await?.parse()?;

    let mut prv_key_data_args = Vec::with_capacity(prv_keys_len);
    for _ in 0..prv_keys_len {
        let bip39_mnemonic = Secret::from(Mnemonic::random(mnemonic_phrase_word_count, Language::default())?.phrase());

        let prv_key_data_create_args = PrvKeyDataCreateArgs::new(None, None, bip39_mnemonic); // can be optimized with Rc<WalletSecret>
        let prv_key_data_id = wallet.create_prv_key_data(&wallet_secret, prv_key_data_create_args).await?;

        prv_key_data_args.push(PrvKeyDataArgs::new(prv_key_data_id, None));
    }

    let additional_xpub_keys_len: usize = term.ask(false, "Enter the number of additional extended public keys: ").await?.parse()?;
    let mut xpub_keys = Vec::with_capacity(additional_xpub_keys_len + prv_keys_len);
    for i in 1..=additional_xpub_keys_len {
        let xpub_key = term.ask(false, &format!("Enter extended public {i} key: ")).await?;
        xpub_keys.push(xpub_key.trim().to_owned());
    }
    let account =
        wallet.create_account_multisig(&wallet_secret, prv_key_data_args, xpub_keys, account_name, minimum_signatures).await?;

    tprintln!(ctx, "\naccount created: {}\n", account.get_list_string()?);
    wallet.select(Some(&account)).await?;
    Ok(())
}

pub(crate) async fn bip32_watch(ctx: &Arc<KaspaCli>, name: Option<&str>) -> Result<()> {
    let term = ctx.term();
    let wallet = ctx.wallet();

    let name = if let Some(name) = name {
        Some(name.to_string())
    } else {
        Some(term.ask(false, "Please enter account name (optional, press <enter> to skip): ").await?.trim().to_string())
    };

    let mut xpub_keys = Vec::with_capacity(1);
    let xpub_key = term.ask(false, "Enter extended public key: ").await?;
    xpub_keys.push(xpub_key.trim().to_owned());

    let wallet_secret = Secret::new(term.ask(true, "Enter wallet password: ").await?.trim().as_bytes().to_vec());
    if wallet_secret.as_ref().is_empty() {
        return Err(Error::WalletSecretRequired);
    }

    let account_create_args_bip32_watch = AccountCreateArgsBip32Watch::new(name, xpub_keys);
    let account = wallet.create_account_bip32_watch(&wallet_secret, account_create_args_bip32_watch).await?;

    tprintln!(ctx, "\naccount created: {}\n", account.get_list_string()?);
    wallet.select(Some(&account)).await?;
    Ok(())
}

pub(crate) async fn multisig_watch(ctx: &Arc<KaspaCli>, name: Option<&str>) -> Result<()> {
    let term = ctx.term();

    let account_name = if let Some(name) = name {
        Some(name.to_string())
    } else {
        Some(term.ask(false, "Please enter account name (optional, press <enter> to skip): ").await?.trim().to_string())
    };

    let term = ctx.term();
    let wallet = ctx.wallet();
    let (wallet_secret, _) = ctx.ask_wallet_secret(None).await?;
    let minimum_signatures: u16 = term.ask(false, "Enter the minimum number of signatures required: ").await?.parse()?;

    let prv_key_data_args = Vec::with_capacity(0);

    let answer = term.ask(false, "Enter the number of extended public keys: ").await?.trim().to_string(); //.parse()?;
    let xpub_keys_len: usize = if answer.is_empty() { 0 } else { answer.parse()? };

    let mut xpub_keys = Vec::with_capacity(xpub_keys_len);
    for i in 1..=xpub_keys_len {
        let xpub_key = term.ask(false, &format!("Enter extended public {i} key: ")).await?;
        xpub_keys.push(xpub_key.trim().to_owned());
    }
    let account =
        wallet.create_account_multisig(&wallet_secret, prv_key_data_args, xpub_keys, account_name, minimum_signatures).await?;

    tprintln!(ctx, "\naccount created: {}\n", account.get_list_string()?);
    wallet.select(Some(&account)).await?;
    Ok(())
}
