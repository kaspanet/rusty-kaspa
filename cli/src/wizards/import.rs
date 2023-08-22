use crate::error::Error;
use crate::imports::*;
use crate::result::Result;
use crate::KaspaCli;
use kaspa_bip32::{Language, Mnemonic};
use kaspa_wallet_core::storage::AccountKind;
use std::sync::Arc;

pub async fn ask(term: &Arc<Terminal>) -> Result<Vec<String>> {
    let mut words: Vec<String> = vec![];
    loop {
        if words.is_empty() {
            tprintln!(term, "Please enter mnemonic (12 or 24 space separated words)");
        } else if words.len() < 12 {
            let remains_for_12 = 12 - words.len();
            let remains_for_24 = 24 - words.len();
            tprintln!(term, "Please enter additional {} or {} words or <enter> to abort", remains_for_12, remains_for_24);
        } else {
            let remains_for_24 = 24 - words.len();
            tprintln!(term, "Please enter additional {} words or <enter> to abort", remains_for_24);
        }
        let text = term.ask(false, "Mnemonic:").await?;
        let list = text.split_whitespace().map(|s| s.to_string()).collect::<Vec<String>>();
        if list.is_empty() {
            return Err(Error::UserAbort);
        }
        words.extend(list);

        if words.len() > 24 || words.len() == 12 || words.len() == 24 {
            break;
        }
    }

    if words.len() > 24 {
        Err("Mnemonic must be 12 or 24 words".into())
    } else {
        Ok(words)
    }
}

pub(crate) async fn import_with_mnemonic(ctx: &Arc<KaspaCli>) -> Result<()> {
    let wallet = ctx.wallet();

    if !wallet.is_open() {
        return Err(Error::WalletIsNotOpen);
    }

    tprintln!(ctx);
    let wallet_secret = Secret::new(ctx.term().ask(true, "Enter wallet password: ").await?.trim().as_bytes().to_vec());
    tprintln!(ctx);
    let mnemonic = ask(&ctx.term()).await?;
    tprintln!(ctx);

    let account_kind = if mnemonic.len() == 24 { AccountKind::Bip32 } else { AccountKind::Legacy };

    let payment_secret = if matches!(account_kind, AccountKind::Bip32) {
        tpara!(
            ctx,
            "\
            \
            If your original wallet has a recovery passphrase, please enter it now.\
            \
            Specifically, this is not a wallet password. This is a secondary payment password\
            used to encrypt your private key. This is known as a 'payment passphrase'\
            'mnemonic password', or a 'recovery passphrase'. If your mnemonic was created\
            with a payment passphrase and you do not enter it now, the import process\
            will generate a different private key.\
            \
            If you do not have a payment password, just press ENTER.\
            \
            ",
        );

        let payment_secret = ctx.term().ask(true, "Enter payment password (optional): ").await?;
        if payment_secret.trim().is_empty() {
            None
        } else {
            Some(Secret::new(payment_secret.trim().as_bytes().to_vec()))
        }
    } else {
        None
    };

    let mnemonic = mnemonic.join(" ");
    let mnemonic = Mnemonic::new(mnemonic.trim(), Language::English)?;

    wallet.import_with_mnemonic(wallet_secret, payment_secret.as_ref(), mnemonic, account_kind).await?;

    Ok(())
}
