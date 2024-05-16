use crate::imports::*;
use kaspa_wallet_core::account::{multisig::MultiSig, Account, BIP32_ACCOUNT_KIND, MULTISIG_ACCOUNT_KIND};

#[derive(Default, Handler)]
#[help("Export transactions, a wallet or a private key")]
pub struct Export;

impl Export {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;

        if argv.is_empty() || argv.first() == Some(&"help".to_string()) {
            tprintln!(ctx, "usage: export [mnemonic]");
            return Ok(());
        }

        let what = argv.first().unwrap();
        match what.as_str() {
            "mnemonic" => {
                let account = ctx.account().await?;
                if account.account_kind() == MULTISIG_ACCOUNT_KIND {
                    let account = account.downcast_arc::<MultiSig>()?;
                    export_multisig_account(ctx, account).await
                } else {
                    export_single_key_account(ctx, account).await
                }
            }
            _ => Err(format!("Invalid argument: {}", what).into()),
        }
    }
}

async fn export_multisig_account(ctx: Arc<KaspaCli>, account: Arc<MultiSig>) -> Result<()> {
    match &account.prv_key_data_ids() {
        None => Err(Error::WatchOnlyAccountNoKeyData),
        Some(v) if v.is_empty() => Err(Error::WatchOnlyAccountNoKeyData),
        Some(prv_key_data_ids) => {
            let wallet_secret = Secret::new(ctx.term().ask(true, "Enter wallet password: ").await?.trim().as_bytes().to_vec());
            if wallet_secret.as_ref().is_empty() {
                return Err(Error::WalletSecretRequired);
            }

            tprintln!(ctx, "required signatures: {}", account.minimum_signatures());
            tprintln!(ctx, "");

            let prv_key_data_store = ctx.store().as_prv_key_data_store()?;
            let mut generated_xpub_keys = Vec::with_capacity(prv_key_data_ids.len());

            for (id, prv_key_data_id) in prv_key_data_ids.iter().enumerate() {
                let prv_key_data = prv_key_data_store.load_key_data(&wallet_secret, prv_key_data_id).await?.unwrap();
                let mnemonic = prv_key_data.as_mnemonic(None).unwrap().unwrap();

                let xpub_key: kaspa_bip32::ExtendedPublicKey<kaspa_bip32::secp256k1::PublicKey> =
                    prv_key_data.create_xpub(None, MULTISIG_ACCOUNT_KIND.into(), 0).await?; // todo it can be done concurrently

                tprintln!(ctx, "");
                tprintln!(ctx, "extended public key {}:", id + 1);
                tprintln!(ctx, "");
                tprintln!(ctx, "{}", ctx.wallet().network_format_xpub(&xpub_key));
                tprintln!(ctx, "");

                tprintln!(ctx, "mnemonic {}:", id + 1);
                tprintln!(ctx, "");
                tprintln!(ctx, "{}", mnemonic.phrase());
                tprintln!(ctx, "");

                generated_xpub_keys.push(xpub_key);
            }
            let test = account.xpub_keys();

            if let Some(keys) = test {
                let additional = keys.iter().filter(|item| !generated_xpub_keys.contains(item));
                additional.enumerate().for_each(|(idx, xpub)| {
                    if idx == 0 {
                        tprintln!(ctx, "additional xpubs: ");
                    }
                    tprintln!(ctx, "{}", ctx.wallet().network_format_xpub(xpub));
                });
            }
            Ok(())
        }
    }
}

async fn export_single_key_account(ctx: Arc<KaspaCli>, account: Arc<dyn Account>) -> Result<()> {
    let prv_key_data_id = account.prv_key_data_id()?;

    let wallet_secret = Secret::new(ctx.term().ask(true, "Enter wallet password: ").await?.trim().as_bytes().to_vec());
    if wallet_secret.as_ref().is_empty() {
        return Err(Error::WalletSecretRequired);
    }

    let prv_key_data = ctx.store().as_prv_key_data_store()?.load_key_data(&wallet_secret, prv_key_data_id).await?;
    let Some(keydata) = prv_key_data else { return Err(Error::KeyDataNotFound) };
    let payment_secret = if keydata.payload.is_encrypted() {
        let payment_secret = Secret::new(ctx.term().ask(true, "Enter payment password: ").await?.trim().as_bytes().to_vec());
        if payment_secret.as_ref().is_empty() {
            return Err(Error::PaymentSecretRequired);
        } else {
            Some(payment_secret)
        }
    } else {
        None
    };

    let prv_key_data = keydata.payload.decrypt(payment_secret.as_ref())?;
    let mnemonic = prv_key_data.as_ref().as_mnemonic()?;

    let xpub_key = keydata.create_xpub(None, BIP32_ACCOUNT_KIND.into(), 0).await?; // todo it can be done concurrently

    tprintln!(ctx, "extended public key:");
    tprintln!(ctx, "");
    tprintln!(ctx, "{}", ctx.wallet().network_format_xpub(&xpub_key));
    tprintln!(ctx, "");

    match mnemonic {
        None => {
            tprintln!(ctx, "mnemonic is not available for this private key");
        }
        Some(mnemonic) if payment_secret.is_none() => {
            tprintln!(ctx, "mnemonic:");
            tprintln!(ctx, "");
            tprintln!(ctx, "{}", mnemonic.phrase());
            tprintln!(ctx, "");
        }
        Some(mnemonic) => {
            tpara!(
                ctx,
                "\
                                IMPORTANT: to recover your private key using this mnemonic in the future \
                                you will need your payment password. Your payment password is permanently associated with \
                                this mnemonic.",
            );
            tprintln!(ctx, "");
            tprintln!(ctx, "mnemonic:");
            tprintln!(ctx, "");
            tprintln!(ctx, "{}", mnemonic.phrase());
            tprintln!(ctx, "");
        }
    };

    Ok(())
}
