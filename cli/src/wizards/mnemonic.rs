use crate::error::Error;
use crate::imports::*;
use crate::result::Result;
use crate::KaspaCli;
use std::sync::Arc;
// use workflow_terminal::{Terminal, tprintln, tpara};

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
        let text = term.ask(false, "Words:").await?;
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

pub(crate) async fn import(ctx: &Arc<KaspaCli>) -> Result<()> {
    let term = ctx.term();
    let wallet = ctx.wallet();

    if !wallet.is_open() {
        return Err(Error::WalletIsNotOpen);
    }

    tprintln!(ctx);
    tpara!(
        ctx,
        "\
        \
        If your original wallet has a recovery passphrase, please enter it now\
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

    let mnemonic = ask(&ctx.term()).await?;

    let _payment_secret = if mnemonic.len() == 24 {
        tprintln!(ctx);
        tprintln!(ctx);
        let payment_secret = term.ask(true, "Enter payment password (optional): ").await?;
        if payment_secret.trim().is_empty() {
            None
        } else {
            Some(Secret::new(payment_secret.trim().as_bytes().to_vec()))
        }
    } else {
        None
    };

    Ok(())
}
