//!
//! Tests for wallet account removal functionality (issue #258).
//!

use crate::api::WalletApi;
use crate::imports::*;
use crate::utxo::balance::Balance;
use crate::wallet::args::WalletCreateArgs;
use kaspa_bip32::WordCount;

/// Helper to create a wallet with a single BIP32 account for testing.
/// Returns (wallet, account, wallet_secret).
async fn setup_wallet_with_account() -> Result<(Arc<Wallet>, Arc<dyn Account>, Secret)> {
    let store = Wallet::resident_store()?;
    let network_id = NetworkId::with_suffix(NetworkType::Testnet, 10);
    let wallet = Wallet::try_with_rpc(None, store, Some(network_id))?.to_arc();

    let wallet_secret = Secret::from("test-secret-258");
    let wallet_args = WalletCreateArgs::new(Some("test-wallet".to_string()), None, EncryptionKind::XChaCha20Poly1305, None, true);

    let (_descriptor, _storage, _mnemonic, account) = wallet
        .create_wallet_with_accounts(&wallet_secret, wallet_args, Some("test-account".to_string()), None, WordCount::Words12, None)
        .await?;

    Ok((wallet, account, wallet_secret))
}

/// Helper to set the balance on an account's UtxoContext directly.
fn set_account_balance(account: &Arc<dyn Account>, balance: Option<Balance>) {
    let mut ctx = account.utxo_context().context();
    ctx.balance = balance;
}

#[tokio::test]
async fn test_remove_account_zero_balance() -> Result<()> {
    let (wallet, account, wallet_secret) = setup_wallet_with_account().await?;
    let account_id = *account.id();

    // Set balance to zero (simulates an activated, scanned account with no funds)
    set_account_balance(&account, Some(Balance::new(0, 0, 0, 0, 0, 0)));

    // Remove should succeed
    wallet.clone().accounts_remove(account_id, wallet_secret.clone()).await?;

    // Verify account no longer exists in storage
    let guard = wallet.guard();
    let guard = guard.lock().await;
    let result = wallet.get_account_by_id(&account_id, &guard).await?;
    assert!(result.is_none(), "Account should be removed from storage");

    Ok(())
}

#[tokio::test]
async fn test_remove_account_with_balance_rejected() -> Result<()> {
    let (wallet, account, wallet_secret) = setup_wallet_with_account().await?;
    let account_id = *account.id();

    // Set non-zero mature balance
    set_account_balance(&account, Some(Balance::new(1_000_000, 0, 0, 1, 0, 0)));

    // Remove should fail with AccountHasBalance
    let result = wallet.clone().accounts_remove(account_id, wallet_secret).await;
    assert!(result.is_err(), "Should reject removal of account with balance");
    let err = result.unwrap_err();
    assert!(err.to_string().contains("non-zero balance"), "Error should mention non-zero balance, got: {}", err);

    Ok(())
}

#[tokio::test]
async fn test_remove_account_with_pending_balance_rejected() -> Result<()> {
    let (wallet, account, wallet_secret) = setup_wallet_with_account().await?;
    let account_id = *account.id();

    // Set non-zero pending balance
    set_account_balance(&account, Some(Balance::new(0, 500_000, 0, 0, 1, 0)));

    // Remove should fail with AccountHasBalance
    let result = wallet.clone().accounts_remove(account_id, wallet_secret).await;
    assert!(result.is_err(), "Should reject removal of account with pending balance");

    Ok(())
}

#[tokio::test]
async fn test_remove_unscanned_account_rejected() -> Result<()> {
    let (wallet, account, wallet_secret) = setup_wallet_with_account().await?;
    let account_id = *account.id();

    // balance is None by default (unscanned) — do NOT set it
    assert!(account.balance().is_none(), "Unscanned account should have None balance");

    // Remove should fail with AccountNotActive
    let result = wallet.clone().accounts_remove(account_id, wallet_secret).await;
    assert!(result.is_err(), "Should reject removal of unscanned account");
    let err = result.unwrap_err();
    assert!(err.to_string().contains("not active"), "Error should mention not active, got: {}", err);

    Ok(())
}

#[tokio::test]
async fn test_remove_nonexistent_account_rejected() -> Result<()> {
    let (wallet, _account, wallet_secret) = setup_wallet_with_account().await?;

    // Use a random account ID
    let fake_id = AccountId::from_hex("deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef")?;

    let result = wallet.clone().accounts_remove(fake_id, wallet_secret).await;
    assert!(result.is_err(), "Should reject removal of nonexistent account");
    let err = result.unwrap_err();
    assert!(err.to_string().contains("not found"), "Error should mention not found, got: {}", err);

    Ok(())
}

#[tokio::test]
async fn test_remove_account_cleans_orphaned_keys() -> Result<()> {
    let (wallet, account, wallet_secret) = setup_wallet_with_account().await?;
    let account_id = *account.id();

    // Get the prv_key_data_ids before removal
    let prv_key_data_ids = account.to_storage()?.prv_key_data_ids;

    // Set balance to zero so removal succeeds
    set_account_balance(&account, Some(Balance::new(0, 0, 0, 0, 0, 0)));

    // Remove the account
    wallet.clone().accounts_remove(account_id, wallet_secret.clone()).await?;

    // Verify private key data was also removed (since no other accounts reference it)
    let prv_key_data_store = wallet.store().as_prv_key_data_store()?;
    for prv_key_data_id in prv_key_data_ids.iter() {
        let key_data = prv_key_data_store.load_key_data(&wallet_secret, prv_key_data_id).await?;
        assert!(key_data.is_none(), "Orphaned private key data should be removed");
    }

    Ok(())
}
