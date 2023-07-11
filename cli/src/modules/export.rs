use crate::imports::*;

#[derive(Default, Handler)]
#[help("Export transactions, a wallet or a private key")]
pub struct Export;

impl Export {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;

        if argv.is_empty() || argv.get(0) == Some(&"help".to_string()) {
            tprintln!(ctx, "Usage: export [mnemonic]");
            return Ok(());
        }

        let what = argv.get(0).unwrap();
        match what.as_str() {
            "mnemonic" => {
                let account = ctx.account().await?;
                let prv_key_data_id = account.prv_key_data_id;

                let wallet_secret = Secret::new(ctx.term().ask(true, "Enter wallet password: ").await?.trim().as_bytes().to_vec());
                if wallet_secret.as_ref().is_empty() {
                    return Err(Error::WalletSecretRequired);
                }

                let access_ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(wallet_secret));
                let prv_key_data = ctx.store().as_prv_key_data_store()?.load_key_data(&access_ctx, &prv_key_data_id).await?;
                if let Some(keydata) = prv_key_data {
                    let payment_secret = if keydata.payload.is_encrypted() {
                        let payment_secret =
                            Secret::new(ctx.term().ask(true, "Enter payment password: ").await?.trim().as_bytes().to_vec());
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
                    if let Some(mnemonic) = mnemonic {
                        if payment_secret.is_none() {
                            tprintln!(ctx, "mnemonic:");
                            tprintln!(ctx, "");
                            tprintln!(ctx, "{}", mnemonic.phrase());
                            tprintln!(ctx, "");
                        } else {
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
                    } else {
                        tprintln!(ctx, "mnemonic is not available for this private key");
                    }

                    Ok(())
                } else {
                    Err(Error::KeyDataNotFound)
                }
            }
            _ => Err(format!("Invalid argument: {}", what).into()),
        }
    }
}
