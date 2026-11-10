//! A simple file-backed wallet: a list of Schnorr keys, receiving addresses derived from them, and transaction
//! building/signing over the outputs the node reports for those addresses.
//!
//! PRE-RELEASE: keys are stored in `wallet.json` UNENCRYPTED. Do not keep real funds in this wallet until encryption
//! is implemented (see the "Encrypt Wallet" menu entry).

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use aero_addresses::{Address, Prefix, Version};
use aero_consensus_core::{
    constants::{SOMPI_PER_AERO, TX_VERSION},
    sign::{Signed, sign_with_multiple_v2},
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{ComputeCommit, MutableTransaction, Transaction, TransactionInput, TransactionOutput},
};
use aero_txscript::pay_to_address_script;
use itertools::Itertools;
use secp256k1::Keypair;
use serde::{Deserialize, Serialize};

use crate::node::WalletUtxo;

/// Minimum standard relay fee is 100 sompi/gram; 110 leaves a margin (same as the `rothschild` tool).
const FEE_RATE: u64 = 110;
/// Mempool minimum relay fee uses post-Toccata cofactors immediately, so normalize accordingly.
const NORMALIZED_TRANSIENT_BYTE_FACTOR: u64 = 2;
/// Outputs below this are not created: change smaller than this is added to the fee instead.
const DUST: u64 = 1_000;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KeyEntry {
    pub secret_hex: String,
    pub label: String,
    pub created: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SentTx {
    pub txid: String,
    pub time: u64,
    pub amount: u64,
    pub fee: u64,
    pub to: String,
}

#[derive(Serialize, Deserialize, Default, Debug)]
struct WalletFile {
    keys: Vec<KeyEntry>,
    sent: Vec<SentTx>,
}

pub struct Wallet {
    path: PathBuf,
    prefix: Prefix,
    file: WalletFile,
}

pub struct BuiltTx {
    pub tx: Transaction,
    pub fee: u64,
}

pub fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// `%APPDATA%\Aero` on Windows, `~/.aero` elsewhere.
pub fn data_dir() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata).join("Aero");
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    home.join(".aero")
}

impl Wallet {
    /// Loads the wallet from `dir/wallet.json`, creating it (with one receiving address) if it does not exist.
    pub fn load_or_create(dir: &Path, prefix: Prefix) -> Result<Self, String> {
        fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        let path = dir.join(format!("wallet-{}.json", prefix));
        let file = if path.exists() {
            let text = fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
            serde_json::from_str(&text).map_err(|e| format!("wallet file is corrupt: {e}"))?
        } else {
            WalletFile::default()
        };
        let mut wallet = Self { path, prefix, file };
        if wallet.file.keys.is_empty() {
            wallet.new_key("(no label)")?;
        }
        Ok(wallet)
    }

    fn save(&self) -> Result<(), String> {
        let text = serde_json::to_string_pretty(&self.file).map_err(|e| e.to_string())?;
        fs::write(&self.path, text).map_err(|e| format!("cannot write {}: {e}", self.path.display()))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Creates a new key, saves the wallet, and returns its address.
    pub fn new_key(&mut self, label: &str) -> Result<Address, String> {
        let keypair = Keypair::new(secp256k1::SECP256K1, &mut rand::thread_rng());
        let entry = KeyEntry { secret_hex: hex_encode(&keypair.secret_bytes()), label: label.to_string(), created: unix_now() };
        self.file.keys.push(entry);
        self.save()?;
        Ok(self.address_of(&keypair))
    }

    fn address_of(&self, keypair: &Keypair) -> Address {
        let (xonly, _) = keypair.x_only_public_key();
        Address::new(self.prefix, Version::PubKey, &xonly.serialize())
    }

    fn keypairs(&self) -> Vec<Keypair> {
        self.file
            .keys
            .iter()
            .filter_map(|k| hex_decode(&k.secret_hex))
            .filter_map(|bytes| Keypair::from_seckey_slice(secp256k1::SECP256K1, &bytes).ok())
            .collect()
    }

    /// All receiving addresses with their labels, oldest first.
    pub fn addresses(&self) -> Vec<(String, Address)> {
        self.file
            .keys
            .iter()
            .filter_map(|k| {
                let bytes = hex_decode(&k.secret_hex)?;
                let keypair = Keypair::from_seckey_slice(secp256k1::SECP256K1, &bytes).ok()?;
                Some((k.label.clone(), self.address_of(&keypair)))
            })
            .collect()
    }

    pub fn sent(&self) -> &[SentTx] {
        &self.file.sent
    }

    pub fn record_sent(&mut self, sent: SentTx) {
        self.file.sent.push(sent);
        let _ = self.save();
    }

    /// Builds and signs a payment of `amount` sompi to `to`, spending from `spendable` (largest first) and
    /// returning change to the wallet's first address. The fee is computed from the estimated mass.
    pub fn build_payment(&self, spendable: &[WalletUtxo], to: &Address, amount: u64) -> Result<BuiltTx, String> {
        if amount == 0 {
            return Err("The amount to pay must be larger than 0.".into());
        }
        let change_address = self.addresses().into_iter().next().map(|(_, a)| a).ok_or("The wallet has no addresses.")?;

        let mut candidates: Vec<&WalletUtxo> = spendable.iter().collect();
        candidates.sort_by_key(|u| std::cmp::Reverse(u.amount));

        let mut selected: Vec<&WalletUtxo> = Vec::new();
        let mut total = 0u64;
        let mut fee = 0u64;
        for utxo in candidates {
            selected.push(utxo);
            total += utxo.amount;
            fee = required_fee(selected.len(), 2);
            if total >= amount + fee {
                break;
            }
        }
        if selected.is_empty() || total < amount + fee {
            return Err("The amount exceeds your balance (including the transaction fee).".into());
        }

        let mut change = total - amount - fee;
        let mut outputs = vec![TransactionOutput { value: amount, script_public_key: pay_to_address_script(to), covenant: None }];
        if change >= DUST {
            outputs.push(TransactionOutput {
                value: change,
                script_public_key: pay_to_address_script(&change_address),
                covenant: None,
            });
        } else {
            // Not worth an output: it simply becomes extra fee.
            fee += change;
            change = 0;
        }
        let _ = change;

        let inputs = selected
            .iter()
            .map(|u| TransactionInput {
                previous_outpoint: u.outpoint,
                signature_script: vec![],
                sequence: 0,
                compute_commit: ComputeCommit::SigopCount(1.into()),
            })
            .collect_vec();
        let unsigned = Transaction::new_non_finalized(TX_VERSION, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
        let entries = selected.iter().map(|u| u.entry.clone()).collect_vec();
        let mutable = MutableTransaction::with_entries(unsigned, entries);

        let secrets: Vec<[u8; 32]> = self.keypairs().iter().map(|k| k.secret_bytes()).collect();
        match sign_with_multiple_v2(mutable, &secrets) {
            Signed::Fully(signed) => Ok(BuiltTx { tx: signed.tx, fee }),
            Signed::Partially(_) => Err("Could not sign all inputs with the wallet's keys.".into()),
        }
    }
}

/// Fee for a standard transaction with `num_inputs` inputs and `num_outputs` outputs (single-signature).
pub fn required_fee(num_inputs: usize, num_outputs: u64) -> u64 {
    let serialized_bytes = 94 // version, counts, locktime, subnetwork id, gas, payload hash and length
        + 118 * num_inputs as u64 // std input: outpoint + signature script (single signature) + sequence
        + 53 * num_outputs; // std output: value + script public key version/length + max std script public key
    let compute_mass = serialized_bytes + 1000 * num_inputs as u64 + 370 * num_outputs;
    FEE_RATE * compute_mass.max(serialized_bytes * NORMALIZED_TRANSIENT_BYTE_FACTOR)
}

/// Formats sompi as a decimal ARO amount with 8 decimals.
pub fn format_aero(sompi: u64) -> String {
    format!("{}.{:08}", sompi / SOMPI_PER_AERO, sompi % SOMPI_PER_AERO)
}

/// Parses a decimal ARO amount ("1", "1.5", "0.00000001") into sompi.
pub fn parse_aero(text: &str) -> Result<u64, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Enter an amount.".into());
    }
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if fraction.len() > 8 {
        return Err("An amount can have at most 8 decimal places.".into());
    }
    let digits_ok = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    if !digits_ok(whole) || !digits_ok(fraction) || (whole.is_empty() && fraction.is_empty()) {
        return Err("Invalid amount.".into());
    }
    let whole: u64 = if whole.is_empty() { 0 } else { whole.parse().map_err(|_| "Invalid amount.".to_string())? };
    let fraction: u64 =
        if fraction.is_empty() { 0 } else { format!("{fraction:0<8}").parse().map_err(|_| "Invalid amount.".to_string())? };
    whole.checked_mul(SOMPI_PER_AERO).and_then(|w| w.checked_add(fraction)).ok_or_else(|| "The amount is too large.".to_string())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_decode(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_amount_formatting_and_parsing_round_trip() {
        for sompi in [0u64, 1, 99_999_999, 100_000_000, 123_456_789, 10_000_000_000_000_000] {
            assert_eq!(parse_aero(&format_aero(sompi)).unwrap(), sompi);
        }
        assert_eq!(parse_aero("1.5").unwrap(), 150_000_000);
        assert_eq!(parse_aero(".5").unwrap(), 50_000_000);
        assert_eq!(parse_aero("2").unwrap(), 200_000_000);
        assert!(parse_aero("1.123456789").is_err());
        assert!(parse_aero("abc").is_err());
        assert!(parse_aero("").is_err());
        assert!(parse_aero("-1").is_err());
    }

    #[test]
    fn test_hex_round_trip() {
        let bytes = [0u8, 1, 127, 128, 255];
        assert_eq!(hex_decode(&hex_encode(&bytes)).unwrap(), bytes);
        assert!(hex_decode("abc").is_none());
        assert!(hex_decode("zz").is_none());
    }

    #[test]
    fn test_fee_grows_with_inputs() {
        assert!(required_fee(2, 2) > required_fee(1, 2));
        assert!(required_fee(1, 2) > 0);
    }

    #[test]
    fn test_wallet_creates_addresses_with_the_network_prefix() {
        let dir = std::env::temp_dir().join(format!("aero-gui-test-{}", unix_now()));
        let mut wallet = Wallet::load_or_create(&dir, Prefix::Devnet).unwrap();
        let second = wallet.new_key("savings").unwrap();
        assert!(second.to_string().starts_with("aerodev:"));
        assert_eq!(wallet.addresses().len(), 2);
        // Reloading finds the same keys.
        let reloaded = Wallet::load_or_create(&dir, Prefix::Devnet).unwrap();
        assert_eq!(reloaded.addresses().len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }
}
