mod covenants;
mod errors;
mod payload_layout;
mod result;
mod scriptnum;

use covenants::{
    asset_id_for_outpoint, build_mint_tx, build_minter_covenant_script_knat20, build_token_covenant_script_knat20,
    build_token_transfer_tx, try_spk_bytes, CovenantError, NativeAssetOp, NativeAssetOutput, NativeAssetPayload, NativeAssetState,
};
use kaspa_addresses::Prefix;
use kaspa_bip32::{secp256k1::PublicKey, AddressType, ChildNumber, ExtendedPrivateKey, Prefix as KeyPrefix, PrivateKey, SecretKey};
use kaspa_consensus_core::constants::TX_VERSION;
use kaspa_consensus_core::mass::{MassCalculator, NonContextualMasses};
use kaspa_consensus_core::sign::{sign_with_multiple_v2, Signed};
use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
use kaspa_consensus_core::tx::{
    PopulatedTransaction, SignableTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
};
use kaspa_rpc_core::{api::rpc::RpcApi, RpcAddress};
use kaspa_txscript::{extract_script_pub_key_address, pay_to_address_script, pay_to_script_hash_script};
use kaspa_wallet_core::{
    derivation::WalletDerivationManagerTrait,
    prelude::{Language, Mnemonic},
    utils::try_kaspa_str_to_sompi,
};
use kaspa_wallet_keys::derivation::gen1::{PubkeyDerivationManager, WalletDerivationManager};
use kaspa_wrpc_client::{
    client::{ConnectOptions, ConnectStrategy},
    prelude::{NetworkId, NetworkType},
    result::Result,
    KaspaRpcClient, WrpcEncoding,
};
use std::process::ExitCode;
use std::time::Duration;

const ACCOUNT_INDEX: u64 = 1;
const FUNDING_INDEX: u32 = 0;
const GENESIS_INDEX: u32 = 1;
const AUTHORITY_INDEX: u32 = 2;
const NEW_OWNER_INDEX: u32 = 3;

const MINT_COUNT: usize = 3;
const TOKEN_AMOUNT: u64 = 2;
const TOTAL_SUPPLY: u64 = 10;
const GENESIS_KAS: &str = "5";
const TOKEN_KAS: &str = "0.5";
const AUTH_KAS: &str = "1";

struct SpendableUtxo {
    outpoint: TransactionOutpoint,
    entry: UtxoEntry,
}

struct WalletContext {
    receive_prv_generator: ExtendedPrivateKey<SecretKey>,
    change_prv_generator: ExtendedPrivateKey<SecretKey>,
    hd_wallet: WalletDerivationManager,
}

impl WalletContext {
    fn from_mnemonic_str(mnemonic_str: &str) -> Self {
        let mnemonic = Mnemonic::new(mnemonic_str, Language::English).expect("Error: provided <mnemonic> isn't a valid mnemonic.");
        let master_xprv =
            ExtendedPrivateKey::<SecretKey>::new(mnemonic.to_seed("")).expect("Error while building xprv from <mnemonic>.");

        let hd_wallet =
            WalletDerivationManager::from_master_xprv(master_xprv.to_string(KeyPrefix::XPRV).as_str(), false, ACCOUNT_INDEX, None)
                .expect("Error while getting hd wallet from xprv.");

        let receive_prv_generator: ExtendedPrivateKey<SecretKey> = master_xprv
            .clone()
            .derive_path(
                &WalletDerivationManager::build_derivate_path(false, ACCOUNT_INDEX, None, Some(kaspa_bip32::AddressType::Receive))
                    .expect("Error while building derivate path."),
            )
            .expect("Error while derive path from xprv.");

        let change_prv_generator: ExtendedPrivateKey<SecretKey> = master_xprv
            .clone()
            .derive_path(
                &WalletDerivationManager::build_derivate_path(false, ACCOUNT_INDEX, None, Some(kaspa_bip32::AddressType::Change))
                    .expect("Error while building derivate path."),
            )
            .expect("Error while derive path from xprv.");

        WalletContext { receive_prv_generator, change_prv_generator, hd_wallet }
    }

    fn get_pubkey_at_index(&self, index: u32, address_type: AddressType) -> PublicKey {
        let derivation_manager = match address_type {
            AddressType::Receive => self.hd_wallet.receive_pubkey_manager(),
            AddressType::Change => self.hd_wallet.change_pubkey_manager(),
        };

        derivation_manager.derive_pubkey(index).expect("Error while derive first pubkey.")
    }

    fn get_prvkey_at_index(&self, index: u32, address_type: AddressType) -> impl PrivateKey {
        let xprv_generator = match address_type {
            AddressType::Receive => &self.receive_prv_generator,
            AddressType::Change => &self.change_prv_generator,
        };

        xprv_generator
            .derive_child(ChildNumber::new(index, false).expect("Error while getting child number"))
            .expect("Error while derive first pubkey.")
            .private_key()
            .to_owned()
    }
}

#[tokio::main]
/// cargo run -p kaspa-wrpc-covenants-native-asset -- <mnemonic>
async fn main() -> ExitCode {
    let mnemonic_arg = std::env::args().nth(1).expect("Error: a <mnemonic> argument was not provided.");
    let wallet_ctx = WalletContext::from_mnemonic_str(&mnemonic_arg);

    match create_native_asset_flow(&wallet_ctx).await {
        Ok(_) => {
            println!("Well done! You successfully deployed, minted, and transferred a native asset.");
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!("An error occurred: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn create_native_asset_flow(wallet_ctx: &WalletContext) -> Result<()> {
    let encoding = WrpcEncoding::Borsh;
    let url = Some("wss://tn12-wrpc.kasia.fyi");
    let resolver = None;
    let network_type = NetworkType::Testnet;
    let mass_calculator = MassCalculator::new_with_consensus_params(&network_type.into());
    let selected_network = Some(NetworkId::with_suffix(network_type, 12));
    let subscription_context = None;
    let client = KaspaRpcClient::new(encoding, url, resolver, selected_network, subscription_context)?;

    let timeout = 5_000;
    let options = ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(Duration::from_millis(timeout)),
        strategy: ConnectStrategy::Fallback,
        ..Default::default()
    };

    let funding_address = PubkeyDerivationManager::create_address(
        &wallet_ctx.get_pubkey_at_index(FUNDING_INDEX, AddressType::Receive),
        Prefix::Testnet,
        false,
    )
    .expect("Cannot get funding address from pubkey");
    let genesis_address = PubkeyDerivationManager::create_address(
        &wallet_ctx.get_pubkey_at_index(GENESIS_INDEX, AddressType::Receive),
        Prefix::Testnet,
        false,
    )
    .expect("Cannot get genesis address from pubkey");
    let authority_address = PubkeyDerivationManager::create_address(
        &wallet_ctx.get_pubkey_at_index(AUTHORITY_INDEX, AddressType::Receive),
        Prefix::Testnet,
        false,
    )
    .expect("Cannot get authority address from pubkey");
    let new_owner_address = PubkeyDerivationManager::create_address(
        &wallet_ctx.get_pubkey_at_index(NEW_OWNER_INDEX, AddressType::Receive),
        Prefix::Testnet,
        false,
    )
    .expect("Cannot get new owner address from pubkey");

    let funding_spk = pay_to_address_script(&funding_address);
    let genesis_spk = pay_to_address_script(&genesis_address);
    let authority_spk = pay_to_address_script(&authority_address);
    let new_owner_spk = pay_to_address_script(&new_owner_address);

    let authority_spk_bytes = try_spk_bytes(&authority_spk).map_err(map_covenant_err)?;
    let owner_spk_bytes = authority_spk_bytes.clone();
    let new_owner_spk_bytes = try_spk_bytes(&new_owner_spk).map_err(map_covenant_err)?;

    let total_minted = TOKEN_AMOUNT.checked_mul(MINT_COUNT as u64).expect("Mint count exceeds token amount range");
    assert!(total_minted <= TOTAL_SUPPLY, "Minted supply exceeds total supply");

    // Build minter first, then derive the token covenant and carry its spk bytes in the payload.
    let minter_covenant_script = build_minter_covenant_script_knat20(&authority_spk_bytes).map_err(map_covenant_err)?;
    let minter_spk = pay_to_script_hash_script(&minter_covenant_script);
    let minter_spk_bytes = try_spk_bytes(&minter_spk).map_err(map_covenant_err)?;

    let token_covenant_script = build_token_covenant_script_knat20(&minter_spk_bytes).map_err(map_covenant_err)?;
    let token_spk = pay_to_script_hash_script(&token_covenant_script);
    let token_spk_bytes = try_spk_bytes(&token_spk).map_err(map_covenant_err)?;

    let minter_address =
        extract_script_pub_key_address(&minter_spk, Prefix::Testnet).expect("Cannot get address from minter covenant spk");
    let token_address =
        extract_script_pub_key_address(&token_spk, Prefix::Testnet).expect("Cannot get address from token covenant spk");

    println!("minter covenant address: {}", minter_address);
    println!("token covenant address: {}", token_address);

    client.connect(Some(options)).await?;

    let genesis_value = try_kaspa_str_to_sompi(GENESIS_KAS.to_string()).expect("Cannot convert genesis amount").unwrap();
    let token_value = try_kaspa_str_to_sompi(TOKEN_KAS.to_string()).expect("Cannot convert token amount").unwrap();
    let auth_value = try_kaspa_str_to_sompi(AUTH_KAS.to_string()).expect("Cannot convert authority amount").unwrap();

    let mut funding_utxos =
        client.get_utxos_by_addresses(vec![RpcAddress::from(funding_address.clone())]).await.expect("Cannot find UTXO by address");

    funding_utxos.sort_by_key(|u| u.utxo_entry.amount);

    let funding_utxo = funding_utxos.iter().find(|utxo| utxo.utxo_entry.amount >= genesis_value).expect("No funding UTXO found");

    let funding_entry: UtxoEntry = funding_utxo.utxo_entry.clone().into();
    let funding_outpoint: TransactionOutpoint = funding_utxo.outpoint.into();

    let authority_count = MINT_COUNT + 1;
    let outputs_total = genesis_value + auth_value * authority_count as u64;
    assert!(funding_entry.amount > outputs_total, "Funding UTXO does not cover requested outputs");

    let funding_input = TransactionInput::new(funding_outpoint, vec![], 0, 1);
    let mut funding_outputs = vec![TransactionOutput::new(genesis_value, genesis_spk.clone())];
    for _ in 0..authority_count {
        funding_outputs.push(TransactionOutput::new(auth_value, authority_spk.clone()));
    }

    let mut temp_outputs = funding_outputs.clone();
    let temp_change_value = funding_entry.amount.saturating_sub(outputs_total);
    if temp_change_value > 0 {
        temp_outputs.push(TransactionOutput::new(temp_change_value, funding_spk.clone()));
    }

    let temp_tx = Transaction::new(TX_VERSION, vec![funding_input.clone()], temp_outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
    let temp_tx = PopulatedTransaction::new(&temp_tx, vec![funding_entry.clone()]);
    let mass = calc_mass(&mass_calculator, temp_tx);

    let change_value = funding_entry.amount.saturating_sub(outputs_total).saturating_sub(mass);
    assert!(change_value > 0, "Funding UTXO does not cover fees");
    funding_outputs.push(TransactionOutput::new(change_value, funding_spk));

    let funding_tx = Transaction::new(TX_VERSION, vec![funding_input], funding_outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
    let funding_prvkey = wallet_ctx.get_prvkey_at_index(FUNDING_INDEX, AddressType::Receive).to_bytes();
    let funding_tx = sign_and_finalize(funding_tx, vec![funding_entry.clone()], &[funding_prvkey]);
    let funding_tx_id = client.submit_transaction((&funding_tx).into(), false).await?;

    let genesis_outpoint = TransactionOutpoint::new(funding_tx.id(), 0);
    let genesis_entry = UtxoEntry::new(funding_tx.outputs[0].value, genesis_spk.clone(), 0, false);

    let genesis_payload = NativeAssetPayload {
        asset_id: asset_id_for_outpoint(&genesis_outpoint),
        authority_spk_bytes,
        token_spk_bytes,
        remaining_supply: TOTAL_SUPPLY,
        op: NativeAssetOp::Mint,
        total_amount: TOKEN_AMOUNT,
        input_amounts: Vec::new(),
        outputs: vec![NativeAssetOutput { amount: TOKEN_AMOUNT, recipient_spk_bytes: owner_spk_bytes.clone() }],
    };

    let genesis_input = TransactionInput::new(genesis_outpoint, vec![], 0, 1);
    let temp_genesis_output = TransactionOutput::new(genesis_entry.amount, minter_spk.clone());
    let temp_genesis_tx = Transaction::new(
        TX_VERSION,
        vec![genesis_input.clone()],
        vec![temp_genesis_output],
        0,
        SUBNETWORK_ID_NATIVE,
        0,
        genesis_payload.encode().unwrap(),
    );
    let temp_genesis_tx = PopulatedTransaction::new(&temp_genesis_tx, vec![genesis_entry.clone()]);
    let genesis_mass = calc_mass(&mass_calculator, temp_genesis_tx);

    let minter_value = genesis_entry.amount.saturating_sub(genesis_mass);
    let minter_output = kaspa_consensus_core::tx::TransactionOutput::new(minter_value, minter_spk.clone());
    let minter_genesis_tx = kaspa_consensus_core::tx::Transaction::new(
        kaspa_consensus_core::constants::TX_VERSION,
        vec![genesis_input],
        vec![minter_output],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE,
        0,
        genesis_payload.encode().unwrap(),
    );
    let genesis_prvkey = wallet_ctx.get_prvkey_at_index(1, kaspa_bip32::AddressType::Receive).to_bytes();
    let minter_genesis_tx = sign_and_finalize(minter_genesis_tx, vec![genesis_entry.clone()], &[genesis_prvkey]);
    let minter_genesis_tx_id = client.submit_transaction((&minter_genesis_tx).into(), true).await?;

    let mut authority_utxos = Vec::with_capacity(authority_count);
    for i in 0..authority_count {
        let output_index = 1 + i as u32;
        let output = funding_tx.outputs[output_index as usize].clone();
        authority_utxos.push(SpendableUtxo {
            outpoint: TransactionOutpoint::new(funding_tx.id(), output_index),
            entry: UtxoEntry::new(output.value, output.script_public_key, 0, false),
        });
    }

    let minter_entry = UtxoEntry::new(minter_genesis_tx.outputs[0].value, minter_spk.clone(), 0, false);
    let mut minter_state = NativeAssetState::from_tx_with_entry_and_grandparent(minter_genesis_tx.clone(), minter_entry, &funding_tx)
        .map_err(map_covenant_err)?;
    let mut minter_parent_tx = minter_genesis_tx.clone();

    let authority_prvkey = wallet_ctx.get_prvkey_at_index(AUTHORITY_INDEX, AddressType::Receive).to_bytes();
    let mut mint_tx_ids = Vec::with_capacity(MINT_COUNT);
    let mut last_token_state = None;

    for (idx, auth_utxo) in authority_utxos.iter().take(MINT_COUNT).enumerate() {
        let auth_input = TransactionInput::new(auth_utxo.outpoint, vec![], 0, 1);
        let auth_entry = auth_utxo.entry.clone();
        let next_payload = minter_state.payload.mint_next(TOKEN_AMOUNT, &owner_spk_bytes).map_err(map_covenant_err)?;

        let mint_tx = build_mint_tx(
            &minter_state,
            &next_payload,
            &minter_spk,
            &token_spk,
            token_value,
            auth_input,
            auth_entry.clone(),
            &minter_covenant_script,
            &mass_calculator,
        )
        .map_err(map_covenant_err)?;
        let mint_tx = sign_and_finalize(mint_tx, vec![minter_state.utxo_entry().clone(), auth_entry.clone()], &[authority_prvkey]);
        let mint_tx_id = client.submit_transaction((&mint_tx).into(), true).await?;
        println!("mint {} tx submitted: {mint_tx_id}", idx + 1);
        mint_tx_ids.push(mint_tx_id);

        let token_entry = UtxoEntry::new(mint_tx.outputs[1].value, token_spk.clone(), 0, false);
        let token_state =
            NativeAssetState::from_tx_with_entry_and_grandparent_at_index(mint_tx.clone(), token_entry, &minter_parent_tx, 1)
                .map_err(map_covenant_err)?;
        last_token_state = Some(token_state);

        let next_minter_entry = UtxoEntry::new(mint_tx.outputs[0].value, minter_spk.clone(), 0, false);
        minter_state = NativeAssetState::from_tx_with_entry_and_grandparent(mint_tx.clone(), next_minter_entry, &minter_parent_tx)
            .map_err(map_covenant_err)?;
        minter_parent_tx = mint_tx;
    }

    let token_state = last_token_state.expect("No token state available for transfer");
    let owner_utxo = &authority_utxos[MINT_COUNT];
    let owner_input = TransactionInput::new(owner_utxo.outpoint, vec![], 0, 1);
    let owner_entry = owner_utxo.entry.clone();
    let transfer_payload = token_state.payload.token_transfer_next(&new_owner_spk_bytes).map_err(map_covenant_err)?;

    let transfer_tx = build_token_transfer_tx(
        &token_state,
        &transfer_payload,
        &token_spk,
        owner_input,
        owner_entry.clone(),
        &token_covenant_script,
        &mass_calculator,
    )
    .map_err(map_covenant_err)?;
    let transfer_tx = sign_and_finalize(transfer_tx, vec![token_state.utxo_entry().clone(), owner_entry.clone()], &[authority_prvkey]);
    let transfer_tx_id = client.submit_transaction((&transfer_tx).into(), true).await?;

    println!("funding tx submitted: {funding_tx_id}");
    println!("minter genesis tx submitted: {minter_genesis_tx_id}");
    println!("transfer tx submitted: {transfer_tx_id}");
    println!("related transaction ids:");
    println!("funding: {funding_tx_id}");
    println!("deploy: {minter_genesis_tx_id}");
    for (index, id) in mint_tx_ids.iter().enumerate() {
        println!("mint-{}: {id}", index + 1);
    }
    println!("transfer: {transfer_tx_id}");

    client.disconnect().await?;
    Ok(())
}

fn calc_mass(calculator: &MassCalculator, tx: PopulatedTransaction<'_>) -> u64 {
    let storage_mass = calculator.calc_contextual_masses(&tx).map(|mass| mass.storage_mass).unwrap_or_default();
    let NonContextualMasses { compute_mass, transient_mass } = calculator.calc_non_contextual_masses(tx.tx);

    println!("storage {}, transient {}", compute_mass, transient_mass);
    // TODO: need to compute mass, but since it depends on fees (output values), it's not trivial
    storage_mass.max(compute_mass).max(transient_mass) + 100
}

fn map_covenant_err(err: CovenantError) -> kaspa_wrpc_client::error::Error {
    kaspa_wrpc_client::error::Error::custom(err)
}

fn sign_and_finalize(tx: Transaction, entries: Vec<UtxoEntry>, privkeys: &[[u8; 32]]) -> Transaction {
    let signable = SignableTransaction::with_entries(tx, entries);
    let mut signed = match sign_with_multiple_v2(signable, privkeys) {
        Signed::Fully(tx) => tx,
        Signed::Partially(tx) => tx,
    };
    signed.tx.finalize();
    signed.tx
}
