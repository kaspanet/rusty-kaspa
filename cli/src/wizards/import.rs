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

pub(crate) async fn import_with_mnemonic(ctx: &Arc<KaspaCli>, account_kind: AccountKind, additional_xpubs: &[String]) -> Result<()> {
    let wallet = ctx.wallet();

    if !wallet.is_open() {
        return Err(Error::WalletIsNotOpen);
    }
    let term = ctx.term();

    tprintln!(ctx);
    let wallet_secret = Secret::new(term.ask(true, "Enter wallet password: ").await?.trim().as_bytes().to_vec());
    tprintln!(ctx);
    let mnemonic = ask(&term).await?;
    tprintln!(ctx);

    match account_kind {
        AccountKind::Legacy if mnemonic.len() != 12 => Err(Error::Custom("wrong mnemonic length".to_owned())),
        AccountKind::Bip32 if mnemonic.len() != 24 => Err(Error::Custom("wrong mnemonic length".to_owned())),

        AccountKind::Legacy | AccountKind::Bip32 | AccountKind::MultiSig => Ok(()),
        _ => Err(Error::Custom("unsupported account kind".to_owned())),
    }?;

    let payment_secret = {
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

        let payment_secret = term.ask(true, "Enter payment password (optional): ").await?;
        if payment_secret.trim().is_empty() {
            None
        } else {
            Some(Secret::new(payment_secret.trim().as_bytes().to_vec()))
        }
    };

    let mnemonic = mnemonic.join(" ");
    let mnemonic = Mnemonic::new(mnemonic.trim(), Language::English)?;

    let account = if !matches!(account_kind, AccountKind::MultiSig) {
        wallet.import_with_mnemonic(wallet_secret, payment_secret.as_ref(), mnemonic, account_kind).await?
    } else {
        let mut mnemonics_secrets = vec![(mnemonic, payment_secret)];
        while matches!(
            term.ask(false, "Do you want to add more mnemonics (type 'y' to approve)?: ").await?.trim(),
            "y" | "Y" | "YES" | "yes"
        ) {
            tprintln!(ctx);
            let mnemonic = ask(&term).await?;
            tprintln!(ctx);
            let payment_secret = term.ask(true, "Enter payment password (optional): ").await?;
            let payment_secret = payment_secret.trim().is_not_empty().then(|| Secret::new(payment_secret.trim().as_bytes().to_vec()));
            let mnemonic = mnemonic.join(" ");
            let mnemonic = Mnemonic::new(mnemonic.trim(), Language::English)?;

            mnemonics_secrets.push((mnemonic, payment_secret));
        }

        let mut additional_xpubs = additional_xpubs.to_vec();
        if additional_xpubs.is_empty() {
            loop {
                let xpub_key = term.ask(false, "Enter extended public key: (empty to skip or stop)").await?;
                if xpub_key.is_empty() {
                    break;
                }
                additional_xpubs.push(xpub_key.trim().to_owned());
            }
        }
        let n_required: u16 = term.ask(false, "Enter the minimum number of signatures required: ").await?.parse()?;

        wallet.import_multisig_with_mnemonic(wallet_secret, mnemonics_secrets, n_required, additional_xpubs).await?
    };

    tprintln!(ctx, "\naccount imported: {}\n", account.get_list_string()?);
    wallet.select(Some(&account)).await?;
    Ok(())
}
