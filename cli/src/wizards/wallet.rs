use crate::cli::KaspaCli;
use crate::imports::*;
use crate::result::Result;
use kaspa_bip32::{Language, Mnemonic, WordCount};
use kaspa_wallet_core::{
    storage::{make_filename, Hint},
    wallet::WalletGuard,
};

pub(crate) async fn create(
    ctx: &Arc<KaspaCli>,
    wallet_guard: Option<WalletGuard<'_>>,
    name: Option<&str>,
    import_with_mnemonic: bool,
) -> Result<()> {
    let term = ctx.term();
    let wallet = ctx.wallet();
    let local_guard = ctx.wallet().guard();

    let guard = match wallet_guard {
        Some(locked_guard) => locked_guard,
        None => local_guard.lock().await,
    };
    // TODO @aspect
    let word_count = WordCount::Words12;

    if let Err(err) = wallet.network_id() {
        tprintln!(ctx);
        tprintln!(ctx, "Before creating a wallet, you need to select a Kaspa network.");
        tprintln!(ctx, "Please use 'network <name>' command to select a network.");
        tprintln!(ctx, "Currently available networks are 'mainnet', 'testnet-10' and 'testnet-11'");
        tprintln!(ctx);
        return Err(err.into());
    }
    let filename = make_filename(&name.map(String::from), &None);
    if wallet.exists(Some(&filename)).await? {
        tprintln!(ctx, "{}", style("WARNING - A previously created wallet already exists!").red().to_string());
        tprintln!(ctx, "NOTE: You can create a differently named wallet by using 'wallet create <name>'");
        tprintln!(ctx);

        let overwrite =
            term.ask(false, "Are you sure you want to overwrite it (type 'y' to approve)?: ").await?.trim().to_string().to_lowercase();
        if overwrite.ne("y") {
            return Ok(());
        }
    }

    let account_name = term.ask(false, "Default account title: ").await?.trim().to_string();
    let account_name = account_name.is_not_empty().then_some(account_name);

    tpara!(
        ctx,
        "\n\
        \"Phishing hint\" is a secret word or a phrase that is displayed \
        when you open your wallet. If you do not see the hint when opening \
        your wallet, you may be accessing a fake wallet designed to steal \
        your private key. If this occurs, stop using the wallet immediately, \
        check the browser URL domain name and seek help on social networks \
        (Kaspa Discord or Telegram). \
        \n\
        ",
    );

    let hint = term.ask(false, "Create phishing hint (optional, press <enter> to skip): ").await?.trim().to_string();
    let hint = hint.is_not_empty().then_some(hint).map(Hint::from);
    //if hint.is_empty() { None } else { Some(hint) };

    let wallet_secret = Secret::new(term.ask(true, "Enter wallet encryption password: ").await?.trim().as_bytes().to_vec());
    if wallet_secret.as_ref().is_empty() {
        return Err(Error::WalletSecretRequired);
    }
    let wallet_secret_validate =
        Secret::new(term.ask(true, "Re-enter wallet encryption password: ").await?.trim().as_bytes().to_vec());
    if wallet_secret_validate.as_ref() != wallet_secret.as_ref() {
        return Err(Error::WalletSecretMatch);
    }

    tprintln!(ctx, "");
    if import_with_mnemonic {
        tpara!(
            ctx,
            "\
            \
            If your original wallet has a bip39 recovery passphrase, please enter it now.\
            \
            Specifically, this is not a wallet password. This is a secondary mnemonic passphrase\
            used to encrypt your mnemonic. This is known as a 'payment passphrase'\
            'mnemonic passphrase', or a 'recovery passphrase'. If your mnemonic was created\
            with a payment passphrase and you do not enter it now, the import process\
            will generate a different private key.\
            \
            If you do not have a bip39 recovery passphrase, press ENTER.\
            \
            ",
        );
    } else {
        tpara!(
            ctx,
            "\
            PLEASE NOTE: The optional bip39 mnemonic passphrase, if provided, will be required to \
            issue transactions. This passphrase will also be required when recovering your wallet \
            in addition to your private key or mnemonic. If you lose this passphrase, you will not \
            be able to use or recover your wallet! \
            \
            If you do not want to use bip39 recovery passphrase, press ENTER.\
            ",
        );
    }

    let payment_secret = term.ask(true, "Enter bip39 mnemonic passphrase (optional): ").await?;
    let payment_secret =
        if payment_secret.trim().is_empty() { None } else { Some(Secret::new(payment_secret.trim().as_bytes().to_vec())) };

    if let Some(payment_secret) = payment_secret.as_ref() {
        let payment_secret_validate =
            Secret::new(term.ask(true, "Please re-enter mnemonic passphrase: ").await?.trim().as_bytes().to_vec());
        if payment_secret_validate.as_ref() != payment_secret.as_ref() {
            return Err(Error::PaymentSecretMatch);
        }
    }

    tprintln!(ctx, "");

    let prv_key_data_args = if import_with_mnemonic {
        let words = crate::wizards::import::prompt_for_mnemonic(&term).await?;
        PrvKeyDataCreateArgs::new(None, payment_secret.clone(), Secret::from(words.join(" ")))
    } else {
        PrvKeyDataCreateArgs::new(
            None,
            payment_secret.clone(),
            Secret::from(Mnemonic::random(word_count, Language::default())?.phrase()),
        )
    };

    let mnemonic_phrase = prv_key_data_args.mnemonic.clone();

    let notifier = ctx.notifier().show(Notification::Processing).await;

    // suspend commits for multiple operations
    wallet.store().batch().await?;

    let wallet_args = WalletCreateArgs::new(name.map(String::from), None, EncryptionKind::XChaCha20Poly1305, hint, true);
    let (_wallet_descriptor, storage_descriptor) = ctx.wallet().create_wallet(&wallet_secret, wallet_args).await?;
    let prv_key_data_id = wallet.create_prv_key_data(&wallet_secret, prv_key_data_args).await?;

    let account_args = AccountCreateArgsBip32::new(account_name, None);
    let account = wallet.create_account_bip32(&wallet_secret, prv_key_data_id, payment_secret.as_ref(), account_args).await?;

    // flush data to storage
    wallet.store().flush(&wallet_secret).await?;

    notifier.hide();

    if !import_with_mnemonic {
        tprintln!(ctx, "");
        tprintln!(ctx, "---");
        tprintln!(ctx, "");
        tprintln!(ctx, "{}", style("IMPORTANT:").red());
        tprintln!(ctx, "");

        tpara!(
            ctx,
            "Your mnemonic phrase allows you to re-create your private key. \
            The person who has access to this mnemonic will have full control of \
            the Kaspa stored in it. Keep your mnemonic safe. Write it down and \
            store it in a safe, preferably in a fire-resistant location. Do not \
            store your mnemonic on this computer or a mobile device. This wallet \
            will never ask you for this mnemonic phrase unless you manually \
            initiate a private key recovery. \
            ",
        );

        // descriptor

        ["", "Never share your mnemonic with anyone!", "---", "", "Your default wallet account mnemonic:", mnemonic_phrase.as_str()?]
            .into_iter()
            .for_each(|line| term.writeln(line));
    }

    term.writeln("");
    term.writeln(format!("Your wallet is stored in: {}", storage_descriptor));
    term.writeln("");

    let receive_address = account.receive_address()?;
    term.writeln("Your default account deposit address:");
    term.writeln(style(receive_address).blue().to_string());
    term.writeln("");

    wallet.open(&wallet_secret, name.map(String::from), WalletOpenArgs::default_with_legacy_accounts(), &guard).await?;
    wallet.activate_accounts(None, &guard).await?;

    Ok(())
}
