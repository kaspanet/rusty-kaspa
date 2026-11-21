use crate::protoserialization::{
    PartiallySignedInput, PartiallySignedTransaction, PubKeySignaturePair, TransactionMessage, TransactionOutput,
};
use kaspa_bip32::{DerivationPath, ExtendedPrivateKey, ExtendedPublicKey, Prefix, secp256k1};
use kaspa_consensus_core::hashing::sighash::{SigHashReusedValuesUnsync, calc_schnorr_signature_hash};
use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
use kaspa_consensus_core::network::NetworkType;
use kaspa_consensus_core::tx::{PopulatedTransaction, Transaction, UtxoEntry, VerifiableTransaction};
use kaspa_rpc_core::{RpcScriptPublicKey, RpcTransaction};
use kaspa_wallet_keys::derivation::gen1::WalletDerivationManager;
use prost::Message;
use std::str::FromStr;
use tonic::Status;

pub const KDUB: Prefix = Prefix::from_parts_unchecked("kdub", 0x038b41ba);
pub const KSUB: Prefix = Prefix::from_parts_unchecked("ksub", 0x0390467d);

pub struct InputSource {
    pub amount: u64,
    pub script_public_key: RpcScriptPublicKey,
    pub derivation_path: String,
}

pub fn extended_public_key_prefix(network_type: NetworkType) -> Prefix {
    match network_type {
        NetworkType::Mainnet => Prefix::KPUB,
        NetworkType::Testnet => Prefix::KTUB,
        NetworkType::Devnet => KDUB,
        NetworkType::Simnet => KSUB,
    }
}

pub fn derivation_path(is_change: bool, index: u32) -> String {
    format!("m/{}/{index}", u8::from(is_change))
}

pub fn account_extended_private_key(
    master: ExtendedPrivateKey<secp256k1::SecretKey>,
    account_index: u64,
) -> Result<ExtendedPrivateKey<secp256k1::SecretKey>, Status> {
    let path = WalletDerivationManager::build_derivate_path(false, account_index, None, None)
        .map_err(|err| Status::internal(err.to_string()))?;
    master.derive_path(&path).map_err(|err| Status::internal(err.to_string()))
}

pub fn decode_partially_signed_transaction(bytes: &[u8]) -> Result<PartiallySignedTransaction, Status> {
    PartiallySignedTransaction::decode(bytes).map_err(|err| Status::invalid_argument(err.to_string()))
}

pub fn unsigned_partially_signed_transaction(
    tx: RpcTransaction,
    inputs: Vec<InputSource>,
    account_xpub: &ExtendedPublicKey<secp256k1::PublicKey>,
    network_type: NetworkType,
) -> Result<PartiallySignedTransaction, Status> {
    if inputs.len() != tx.inputs.len() {
        return Err(Status::internal("unbalanced inputs"));
    }
    let prefix = extended_public_key_prefix(network_type);
    let partially_signed_inputs = inputs
        .into_iter()
        .map(|input| {
            let path = DerivationPath::from_str(&input.derivation_path).map_err(|err| Status::internal(err.to_string()))?;
            let xpub = account_xpub.clone().derive_path(&path).map_err(|err| Status::internal(err.to_string()))?;
            Ok(PartiallySignedInput {
                redeem_script: vec![],
                prev_output: Some(TransactionOutput { value: input.amount, script_public_key: Some(input.script_public_key.into()) }),
                minimum_signatures: 1,
                pub_key_signature_pairs: vec![PubKeySignaturePair {
                    extended_pub_key: xpub.to_string(Some(prefix)),
                    signature: vec![],
                }],
                derivation_path: input.derivation_path,
            })
        })
        .collect::<Result<Vec<_>, Status>>()?;
    Ok(PartiallySignedTransaction { tx: Some(TransactionMessage::from(tx)), partially_signed_inputs })
}

pub fn is_fully_signed(pst: &PartiallySignedTransaction) -> bool {
    pst.partially_signed_inputs.iter().all(|input| {
        let signatures = input.pub_key_signature_pairs.iter().filter(|pair| !pair.signature.is_empty()).count();
        u32::try_from(signatures).is_ok_and(|signatures| signatures >= input.minimum_signatures)
    })
}

pub fn sign_partially_signed_transaction(
    mut pst: PartiallySignedTransaction,
    account_xprv: &ExtendedPrivateKey<secp256k1::SecretKey>,
    network_type: NetworkType,
) -> Result<PartiallySignedTransaction, Status> {
    let Some(tx) = pst.tx.as_ref() else { return Err(Status::invalid_argument("missing transaction")) };
    if pst.partially_signed_inputs.len() != tx.inputs.len() {
        return Err(Status::invalid_argument("unbalanced inputs"));
    }
    if is_fully_signed(&pst) {
        return Ok(pst);
    }
    if pst.partially_signed_inputs.iter().any(|input| input.pub_key_signature_pairs.len() > 1) {
        return Err(Status::unimplemented("multisig signing is not supported"));
    }

    let Some(tx) = pst.tx.as_mut() else { return Err(Status::invalid_argument("missing transaction")) };
    let mut entries = Vec::with_capacity(pst.partially_signed_inputs.len());
    for (partially_signed_input, tx_input) in pst.partially_signed_inputs.iter().zip(tx.inputs.iter_mut()) {
        tx_input.sig_op_count = u32::try_from(partially_signed_input.pub_key_signature_pairs.len())
            .map_err(|err| Status::invalid_argument(err.to_string()))?;
        let prev_output =
            partially_signed_input.prev_output.clone().ok_or_else(|| Status::invalid_argument("missing previous output"))?;
        let script_public_key = RpcScriptPublicKey::try_from(
            prev_output.script_public_key.ok_or_else(|| Status::invalid_argument("missing script public key"))?,
        )?;
        entries.push(UtxoEntry::new(prev_output.value, script_public_key, 0, false, None));
    }

    let transaction =
        Transaction::try_from(RpcTransaction::try_from(tx.clone())?).map_err(|err| Status::invalid_argument(err.to_string()))?;
    let populated = PopulatedTransaction::new(&transaction, entries);
    let reused_values = SigHashReusedValuesUnsync::new();
    let prefix = extended_public_key_prefix(network_type);

    let mut signed = false;
    for (input_index, partially_signed_input) in pst.partially_signed_inputs.iter_mut().enumerate() {
        let path = DerivationPath::from_str(&partially_signed_input.derivation_path)
            .map_err(|err| Status::invalid_argument(err.to_string()))?;
        let derived = account_xprv.clone().derive_path(&path).map_err(|err| Status::invalid_argument(err.to_string()))?;
        let extended_pub_key = derived.public_key().to_string(Some(prefix));
        for pair in partially_signed_input.pub_key_signature_pairs.iter_mut().filter(|pair| pair.extended_pub_key == extended_pub_key)
        {
            pair.signature = schnorr_signature(&populated, input_index, derived.private_key(), &reused_values);
            signed = true;
        }
    }

    if !signed {
        return Err(Status::invalid_argument("public key doesn't match any of the transaction public keys"));
    }
    Ok(pst)
}

fn schnorr_signature(
    tx: &impl VerifiableTransaction,
    input_index: usize,
    private_key: &secp256k1::SecretKey,
    reused_values: &SigHashReusedValuesUnsync,
) -> Vec<u8> {
    let hash = calc_schnorr_signature_hash(tx, input_index, SIG_HASH_ALL, reused_values);
    let message = secp256k1::Message::from_digest(hash.as_bytes());
    let keypair = secp256k1::Keypair::from_secret_key(secp256k1::SECP256K1, private_key);
    let signature = keypair.sign_schnorr(message);
    signature.as_ref().iter().copied().chain([SIG_HASH_ALL.to_u8()]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::extract_tx;
    use kaspa_bip32::{Language, Mnemonic};
    use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
    use kaspa_rpc_core::{RpcScriptVec, RpcTransactionId, RpcTransactionInput, RpcTransactionOutpoint, RpcTransactionOutput};
    use kaspa_txscript::caches::Cache;
    use kaspa_txscript::{EngineCtx, TxScriptEngine};
    use kaspa_utils::hex::FromHex;
    use kaspa_wallet_core::account::variants::bip32::BIP32_ACCOUNT_KIND;
    use kaspa_wallet_core::derivation::{create_address, create_xpub_from_mnemonic};
    use kaspa_wallet_core::prelude::{AccountKind, AddressPrefix};
    use kaspa_wallet_keys::derivation::traits::WalletDerivationManagerTrait;
    use tonic::Code;

    const MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
    const OTHER_MNEMONIC: &str = "legal winner thank year wave sausage worth useful legal winner thank yellow";

    const TESTNET_RECEIVE_3_XPUB: &str =
        "ktub27GJJrooPCrRVSF1HhXez24rPGjK2hetsvmE8ry6KFZ6otR3w3bsCi944HaBUt1sqzpD8N1N4iSaeWsz2zQZH6Sh1NhMZnDMWPXE7yHxJ7q";
    const TESTNET_CHANGE_1_XPUB: &str =
        "ktub27TtiZGN1qKdCeFDGkz2FMq9GPr6Hm6Ts1wMcNCN2oM7rtow84pQPBpD4H97JmNgcZFfcHRXmGzUffgdJytNGv4VK3s8QpkDc3AeFuGJyz7";
    const MAINNET_RECEIVE_3_XPUB: &str =
        "kpub2NaNd7x8RE8WKP29TwLmfGFu8cr68SurzgUVchen9TQ87bPeGCgDdVH2GrUBPveQ1DBkAb4n3dHewSPRWeuup4GKzyFdsUuwveLfX5q3adS";
    const MAINNET_CHANGE_1_XPUB: &str =
        "kpub2Nmy2pQh3rbi2b2MSzo8vc2C1jxsPWMRymed6Ct3s1C9AbnXTDtkoxxBGr37Dp1CmmdCeWUwkBqYxbC4nePiost8JeRQiXSp2Hz5f2R7RN4";

    const MAINNET_ACCOUNT_XPUB: &str =
        "kpub2JSci5UuMxrpZk5KV8UhQ5j4eBYe4WmgahxirQ1jcdjatLr1jJg5YoYpHmByYFHvJBPVubmdvXbQG4Gmy2koq9bYBLFYoGvD5NSrCCffFAr";
    const TESTNET_ACCOUNT_XPUB: &str =
        "ktub238YPpLaKwajjoJBJtfaiqY1tqRrxmWiTxFTNZL3nRtZadsRQ9bj82Qr5CHydCfQ8y1xsNiDwckKy8mLVNFTJBmuBjhGVaDcf7dQoAQecWU";
    const DEVNET_ACCOUNT_XPUB: &str =
        "kdub56yVWXFeHXQPkqTHfgzfGaBHhdne2HM7CoLW3VJYcgNkTxWYfCWP8ckaU38kgev5b8fFfbA3xyER7vex6X2MZteiYTfa9FKtPnf3sF6xuNf";
    const SIMNET_ACCOUNT_XPUB: &str =
        "ksub8LEaH4WApSznhXCxSfiYvMNK6RDe47BFQCgf77iz8w3ssq4HYaVpTEvNsUxKk4Vik7rwqYQADetnDMrM4X1GEiNLLMV65squGKiWdgxaThM";

    const GO_SIGNED_PST: &str = "0a9b0112280a240a220a2011111111111111111111111111111111111111111111111111111111111111112001122a0a260a220a202222222222222222222222222222222222222222222222222222222222222222100120011a2b0880d7cc5a12240a222094f42b8818f7a515339b36990612f98a2e292a148ea3b9c16150c3627d97de08ac2a160a14000000000000000000000000000000000000000012ed01122b0880a3c34712240a22201b6739774cb9d28e704d25c0b0c75ecebbb37966c7e548abe730cc490baa50e5ac180122b4010a6f6b7475623237474a4a726f6f5043725256534631486858657a32347250476a4b3268657473766d45387279364b465a366f74523377336273436939343448614255743173717a7044384e314e346953616557737a327a515a48365368314e684d5a6e444d57505845377948784a37711241b0e3270ec6c7a1b89b0de62bf346e2cfe6d42821a89c2c1e683d3089b8d27259cdf7b9382e6a6d0151f4b7d51bb36c45c194d4a31097ea8fd1a3c234c4699e15012a056d2f302f3312ed01122b0880e1eb1712240a2220a3fd6fea98fa5fba910fa2f058af133b7dbd574edecf0d6ba16e44502a4c4957ac180122b4010a6f6b74756232375474695a474e31714b6443654644476b7a32464d713947507236486d36547331774d634e434e326f4d3772746f773834705150427044344839374a6d4e67635a4666634852586d477a55666667644a79744e477634564b33733851706b44633341654675474a797a3712418e7699b6da2437d1928c95da352595c8f249f7b9bfeccad7af1f2002dc265c482b7f729350b2d50f2c756dce2ed99a6565005d478eb2bcbe560bef244d4c9b55012a056d2f312f31";
    const GO_SIGNED_TX_ID: &str = "7f22b637ff2d7b600819c506cd21aaf12b9b724b03aaba4ef679ea379380ea4f";

    fn account_xprv_from(mnemonic: &str) -> ExtendedPrivateKey<secp256k1::SecretKey> {
        let seed = Mnemonic::new(mnemonic, Language::English).unwrap().to_seed("");
        account_extended_private_key(ExtendedPrivateKey::new(seed).unwrap(), 0).unwrap()
    }

    fn account_xprv() -> ExtendedPrivateKey<secp256k1::SecretKey> {
        account_xprv_from(MNEMONIC)
    }

    fn account_xpub() -> ExtendedPublicKey<secp256k1::PublicKey> {
        account_xprv().public_key()
    }

    fn derived_public_key(path: &str) -> secp256k1::PublicKey {
        *account_xpub().derive_path(&DerivationPath::from_str(path).unwrap()).unwrap().public_key()
    }

    fn p2pk(path: &str) -> RpcScriptPublicKey {
        let mut script = vec![0x20];
        script.extend_from_slice(&derived_public_key(path).x_only_public_key().0.serialize());
        script.push(0xac);
        RpcScriptPublicKey::new(0, RpcScriptVec::from(script))
    }

    fn input(byte: u8, index: u32) -> RpcTransactionInput {
        RpcTransactionInput {
            previous_outpoint: RpcTransactionOutpoint { transaction_id: RpcTransactionId::from_bytes([byte; 32]), index },
            signature_script: vec![],
            sequence: 0,
            sig_op_count: 1,
            compute_budget: 0,
            verbose_data: None,
        }
    }

    fn sample_tx() -> RpcTransaction {
        RpcTransaction {
            version: 0,
            inputs: vec![input(0x11, 0), input(0x22, 1)],
            outputs: vec![RpcTransactionOutput {
                value: 190_000_000,
                script_public_key: p2pk("m/0/0"),
                verbose_data: None,
                covenant: None,
            }],
            lock_time: 0,
            subnetwork_id: SUBNETWORK_ID_NATIVE,
            gas: 0,
            payload: vec![],
            storage_mass: 0,
            verbose_data: None,
        }
    }

    fn sample_inputs() -> Vec<InputSource> {
        vec![
            InputSource { amount: 150_000_000, script_public_key: p2pk("m/0/3"), derivation_path: derivation_path(false, 3) },
            InputSource { amount: 50_000_000, script_public_key: p2pk("m/1/1"), derivation_path: derivation_path(true, 1) },
        ]
    }

    fn unsigned(network_type: NetworkType) -> PartiallySignedTransaction {
        unsigned_partially_signed_transaction(sample_tx(), sample_inputs(), &account_xpub(), network_type).unwrap()
    }

    fn extract_and_verify(pst_bytes: &[u8]) -> Transaction {
        let pst = decode_partially_signed_transaction(pst_bytes).unwrap();
        let entries = pst
            .partially_signed_inputs
            .iter()
            .map(|input| {
                let prev_output = input.prev_output.clone().unwrap();
                let script_public_key = RpcScriptPublicKey::try_from(prev_output.script_public_key.unwrap()).unwrap();
                UtxoEntry::new(prev_output.value, script_public_key, 0, false, None)
            })
            .collect::<Vec<_>>();
        let transaction = Transaction::try_from(extract_tx(pst_bytes, false).unwrap()).unwrap();
        let populated = PopulatedTransaction::new(&transaction, entries);
        let sig_cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();
        for (index, tx_input) in transaction.inputs.iter().enumerate() {
            let mut vm = TxScriptEngine::from_transaction_input(
                &populated,
                tx_input,
                index,
                populated.utxo(index).unwrap(),
                EngineCtx::new(&sig_cache).with_reused(&reused_values),
                Default::default(),
            );
            vm.execute().unwrap();
        }
        transaction
    }

    #[tokio::test]
    async fn derivation_matches_wallet_core() {
        let kind = AccountKind::from(BIP32_ACCOUNT_KIND);
        let wallet_core_xpub = create_xpub_from_mnemonic(MNEMONIC, kind, 0).await.unwrap();
        assert_eq!(wallet_core_xpub.to_string(Some(Prefix::KTUB)), account_xpub().to_string(Some(Prefix::KTUB)));

        let manager = WalletDerivationManager::from_extended_public_key(wallet_core_xpub, None).unwrap();
        for index in 0..5 {
            let address = |key| create_address(1, vec![key], AddressPrefix::Testnet, false, Some(kind)).unwrap();
            assert_eq!(
                address(derived_public_key(&derivation_path(false, index))),
                address(manager.derive_receive_pubkey(index).unwrap())
            );
            assert_eq!(
                address(derived_public_key(&derivation_path(true, index))),
                address(manager.derive_change_pubkey(index).unwrap())
            );
        }
    }

    #[test]
    fn unsigned_partially_signed_inputs() {
        let expected_inputs = sample_inputs();
        for (network_type, expected_xpubs) in [
            (NetworkType::Testnet, [TESTNET_RECEIVE_3_XPUB, TESTNET_CHANGE_1_XPUB]),
            (NetworkType::Mainnet, [MAINNET_RECEIVE_3_XPUB, MAINNET_CHANGE_1_XPUB]),
        ] {
            let pst = unsigned(network_type);
            assert_eq!(pst.tx, Some(TransactionMessage::from(sample_tx())));
            assert_eq!(pst.partially_signed_inputs.len(), 2);
            for ((input, expected), expected_xpub) in pst.partially_signed_inputs.iter().zip(&expected_inputs).zip(expected_xpubs) {
                let prev_output = input.prev_output.clone().unwrap();
                assert_eq!(prev_output.value, expected.amount);
                assert_eq!(prev_output.script_public_key, Some(expected.script_public_key.clone().into()));
                assert_eq!(input.minimum_signatures, 1);
                assert_eq!(input.derivation_path, expected.derivation_path);
                assert!(input.redeem_script.is_empty());
                assert_eq!(input.pub_key_signature_pairs.len(), 1);
                assert_eq!(input.pub_key_signature_pairs[0].extended_pub_key, expected_xpub);
                assert!(input.pub_key_signature_pairs[0].signature.is_empty());
            }
            assert_eq!(pst.partially_signed_inputs[0].derivation_path, "m/0/3");
            assert_eq!(pst.partially_signed_inputs[1].derivation_path, "m/1/1");
            assert!(!is_fully_signed(&pst));
        }
    }

    #[test]
    fn rust_signed_transaction_verifies() {
        let signed = sign_partially_signed_transaction(unsigned(NetworkType::Testnet), &account_xprv(), NetworkType::Testnet).unwrap();
        assert!(is_fully_signed(&signed));
        for input in &signed.partially_signed_inputs {
            let signature = &input.pub_key_signature_pairs[0].signature;
            assert_eq!(signature.len(), 65);
            assert_eq!(signature[64], SIG_HASH_ALL.to_u8());
        }
        assert!(signed.tx.as_ref().unwrap().inputs.iter().all(|input| input.sig_op_count == 1));
        extract_and_verify(&signed.encode_to_vec());
    }

    #[test]
    fn go_signed_transaction_verifies() {
        let bytes = Vec::<u8>::from_hex(GO_SIGNED_PST).unwrap();
        let pst = decode_partially_signed_transaction(&bytes).unwrap();
        assert!(is_fully_signed(&pst));
        let transaction = extract_and_verify(&bytes);
        assert_eq!(transaction.id().to_string(), GO_SIGNED_TX_ID);
        for (tx_input, input) in transaction.inputs.iter().zip(&pst.partially_signed_inputs) {
            let expected = [vec![0x41], input.pub_key_signature_pairs[0].signature.clone()].concat();
            assert_eq!(tx_input.signature_script, expected);
        }
    }

    #[test]
    fn wrong_key_does_not_sign() {
        let err = sign_partially_signed_transaction(
            unsigned(NetworkType::Testnet),
            &account_xprv_from(OTHER_MNEMONIC),
            NetworkType::Testnet,
        )
        .unwrap_err();
        assert_eq!(err.code(), Code::InvalidArgument);
        assert_eq!(err.message(), "public key doesn't match any of the transaction public keys");
    }

    #[test]
    fn input_count_mismatch_is_rejected() {
        let mut pst = unsigned(NetworkType::Testnet);
        pst.partially_signed_inputs.pop();
        let err = sign_partially_signed_transaction(pst, &account_xprv(), NetworkType::Testnet).unwrap_err();
        assert_eq!(err.code(), Code::InvalidArgument);

        let mut pst = unsigned(NetworkType::Testnet);
        pst.tx = None;
        let err = sign_partially_signed_transaction(pst, &account_xprv(), NetworkType::Testnet).unwrap_err();
        assert_eq!(err.code(), Code::InvalidArgument);

        let err = unsigned_partially_signed_transaction(sample_tx(), vec![], &account_xpub(), NetworkType::Testnet).unwrap_err();
        assert_eq!(err.code(), Code::Internal);
    }

    #[test]
    fn multisig_input_is_unimplemented() {
        let mut pst = unsigned(NetworkType::Testnet);
        let pair = pst.partially_signed_inputs[0].pub_key_signature_pairs[0].clone();
        pst.partially_signed_inputs[0].pub_key_signature_pairs.push(pair);
        let err = sign_partially_signed_transaction(pst, &account_xprv(), NetworkType::Testnet).unwrap_err();
        assert_eq!(err.code(), Code::Unimplemented);
    }

    #[test]
    fn fully_signed_transaction_is_returned_unchanged() {
        let signed = sign_partially_signed_transaction(unsigned(NetworkType::Testnet), &account_xprv(), NetworkType::Testnet).unwrap();
        let again =
            sign_partially_signed_transaction(signed.clone(), &account_xprv_from(OTHER_MNEMONIC), NetworkType::Testnet).unwrap();
        assert_eq!(again, signed);
    }

    #[test]
    fn decode_rejects_invalid_bytes() {
        let err = decode_partially_signed_transaction(&[0xff, 0xff, 0xff]).unwrap_err();
        assert_eq!(err.code(), Code::InvalidArgument);
    }

    #[test]
    fn prefix_mapping() {
        for (network_type, version, expected) in [
            (NetworkType::Mainnet, 0x038f332e, MAINNET_ACCOUNT_XPUB),
            (NetworkType::Testnet, 0x0390a241, TESTNET_ACCOUNT_XPUB),
            (NetworkType::Devnet, 0x038b41ba, DEVNET_ACCOUNT_XPUB),
            (NetworkType::Simnet, 0x0390467d, SIMNET_ACCOUNT_XPUB),
        ] {
            let prefix = extended_public_key_prefix(network_type);
            assert_eq!(prefix.version(), version);
            assert_eq!(account_xpub().to_string(Some(prefix)), expected);
        }
    }

    #[test]
    fn derivation_path_format() {
        assert_eq!(derivation_path(false, 0), "m/0/0");
        assert_eq!(derivation_path(false, 17), "m/0/17");
        assert_eq!(derivation_path(true, 5), "m/1/5");
    }
}
