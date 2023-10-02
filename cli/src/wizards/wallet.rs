use crate::cli::KaspaCli;
use crate::imports::*;
use crate::result::Result;
use kaspa_wallet_core::runtime::{PrvKeyDataCreateArgs, WalletCreateArgs};
use kaspa_wallet_core::storage::{AccessContextT, AccountKind, Hint};

pub(crate) async fn create(ctx: &Arc<KaspaCli>, name: Option<&str>) -> Result<()> {
    let term = ctx.term();
    let wallet = ctx.wallet();

    if let Err(err) = wallet.network_id() {
        tprintln!(ctx);
        tprintln!(ctx, "Before creating a wallet, you need to select a Kaspa network.");
        tprintln!(ctx, "Please use 'network <name>' command to select a network.");
        tprintln!(ctx, "Currently available networks are 'mainnet', 'testnet-10' and 'testnet-11'");
        tprintln!(ctx);
        return Err(err.into());
    }

    if wallet.exists(name).await? {
        tprintln!(ctx, "{}", style("WARNING - A previously created wallet already exists!").red().to_string());
        tprintln!(ctx, "NOTE: You can create a differently named wallet by using 'wallet create <name>'");
        tprintln!(ctx);

        let overwrite =
            term.ask(false, "Are you sure you want to overwrite it (type 'y' to approve)?: ").await?.trim().to_string().to_lowercase();
        if overwrite.ne("y") {
            return Ok(());
        }
    }

    let account_title = term.ask(false, "Default account title: ").await?.trim().to_string();
    let account_name = account_title.replace(' ', "-").to_lowercase();
    let account_title = account_title.is_not_empty().then_some(account_title);
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
    tpara!(
        ctx,
        "\
        PLEASE NOTE: The optional payment password, if provided, will be required to \
        issue transactions. This password will also be required when recovering your wallet \
        in addition to your private key or mnemonic. If you loose this password, you will not \
        be able to use mnemonic to recover your wallet! \
        ",
    );

    let payment_secret = term.ask(true, "Enter payment password (optional): ").await?;
    let payment_secret =
        if payment_secret.trim().is_empty() { None } else { Some(Secret::new(payment_secret.trim().as_bytes().to_vec())) };

    if let Some(payment_secret) = payment_secret.as_ref() {
        let payment_secret_validate = Secret::new(
            term.ask(true, "Enter payment (private key encryption) password (optional): ").await?.trim().as_bytes().to_vec(),
        );
        if payment_secret_validate.as_ref() != payment_secret.as_ref() {
            return Err(Error::PaymentSecretMatch);
        }
    }

    tprintln!(ctx, "");

    let notifier = ctx.notifier().show(Notification::Processing).await;
    // suspend commits for multiple operations
    wallet.store().batch().await?;

    let account_kind = AccountKind::Bip32;
    let wallet_args = WalletCreateArgs::new(name.map(String::from), None, hint, wallet_secret.clone(), true);
    let prv_key_data_args = PrvKeyDataCreateArgs::new(None, wallet_secret.clone(), payment_secret.clone());
    let account_args = AccountCreateArgs::new(account_name, account_title, account_kind, wallet_secret.clone(), payment_secret);
    let descriptor = ctx.wallet().create_wallet(wallet_args).await?;
    let (prv_key_data_id, mnemonic) = wallet.create_prv_key_data(prv_key_data_args).await?;
    let account = wallet.create_bip32_account(prv_key_data_id, account_args).await?;

    // flush data to storage
    let access_ctx: Arc<dyn AccessContextT> = Arc::new(AccessContext::new(wallet_secret));
    wallet.store().flush(&access_ctx).await?;
    notifier.hide();

    tprintln!(ctx, "");
    tprintln!(ctx, "---");
    tprintln!(ctx, "");
    tprintln!(ctx, "{}", style("IMPORTANT:").red());
    tprintln!(ctx, "");

    tpara!(
        ctx,
        "Your mnemonic phrase allows your to re-create your private key. \
        The person who has access to this mnemonic will have full control of \
        the Kaspa stored in it. Keep your mnemonic safe. Write it down and \
        store it in a safe, preferably in a fire-resistant location. Do not \
        store your mnemonic on this computer or a mobile device. This wallet \
        will never ask you for this mnemonic phrase unless you manually \
        initiate a private key recovery. \
        ",
    );

    // descriptor

    ["", "Never share your mnemonic with anyone!", "---", "", "Your default wallet account mnemonic:", mnemonic.phrase()]
        .into_iter()
        .for_each(|line| term.writeln(line));

    term.writeln("");
    if let Some(descriptor) = descriptor {
        term.writeln(format!("Your wallet is stored in: {}", descriptor));
        term.writeln("");
    }

    let receive_address = account.receive_address()?;
    term.writeln("Your default account deposit address:");
    term.writeln(style(receive_address).blue().to_string());
    term.writeln("");

    Ok(())
}
