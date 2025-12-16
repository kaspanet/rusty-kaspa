#![allow(unused_imports)]

use crate::imports::*;
use faster_hex::hex_string;
use kaspa_addresses::Prefix;
use kaspa_bip32::secp256k1;
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_wallet_core::account::multisig::MultiSig;
use kaspa_wallet_core::account::pskb::finalize_pskt_one_or_more_sig_and_redeem_script;
use kaspa_wallet_pskt::{
    prelude::{lock_script_sig_templating, script_sig_to_address, unlock_utxos_as_pskb, Bundle, SignInputOk, Signature, Signer, PSKT},
    pskt::Inner,
};

#[derive(Default, Handler)]
#[help("Send a Kaspa transaction to a public address")]
pub struct Pskb;

impl Pskb {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, mut argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;

        if !ctx.wallet().is_open() {
            return Err(Error::WalletIsNotOpen);
        }

        if argv.is_empty() {
            return self.display_help(ctx, argv).await;
        }

        let action = argv.remove(0);

        match action.as_str() {
            "create" => {
                if argv.len() < 2 || argv.len() > 3 {
                    return self.display_help(ctx, argv).await;
                }
                let (wallet_secret, payment_secret) = ctx.ask_wallet_secret(None).await?;
                let _ = ctx.notifier().show(Notification::Processing).await;

                let address = Address::try_from(argv.first().unwrap().as_str())?;
                let amount_sompi = try_parse_required_nonzero_kaspa_as_sompi_u64(argv.get(1))?;
                let outputs = PaymentOutputs::from((address, amount_sompi));
                let priority_fee_sompi = try_parse_optional_kaspa_as_sompi_i64(argv.get(2))?.unwrap_or(0);
                let abortable = Abortable::default();

                let account: Arc<dyn Account> = ctx.wallet().account()?;
                let signer = account
                    .pskb_from_send_generator(
                        outputs.into(),
                        // fee_rate
                        None,
                        priority_fee_sompi.into(),
                        None,
                        wallet_secret.clone(),
                        payment_secret.clone(),
                        &abortable,
                    )
                    .await?;

                match signer.serialize() {
                    Ok(encoded) => tprintln!(ctx, "{encoded}"),
                    Err(e) => return Err(e.into()),
                }
            }
            "script" => {
                if argv.len() < 2 || argv.len() > 4 {
                    return self.display_help(ctx, argv).await;
                }
                let subcommand = argv.remove(0);
                let payload = argv.remove(0);
                let account = ctx.wallet().account()?;
                let receive_address = account.receive_address()?;
                let (wallet_secret, payment_secret) = ctx.ask_wallet_secret(None).await?;
                let _ = ctx.notifier().show(Notification::Processing).await;

                let script_sig = match lock_script_sig_templating(payload.clone(), Some(&receive_address.payload)) {
                    Ok(value) => value,
                    Err(e) => {
                        terrorln!(ctx, "{}", e.to_string());
                        return Err(e.into());
                    }
                };

                let script_p2sh = match script_sig_to_address(&script_sig, ctx.wallet().address_prefix()?) {
                    Ok(p2sh) => p2sh,
                    Err(e) => {
                        terrorln!(ctx, "Error generating script address: {}", e.to_string());
                        return Err(e.into());
                    }
                };

                match subcommand.as_str() {
                    "lock" => {
                        let amount_sompi = try_parse_required_nonzero_kaspa_as_sompi_u64(argv.first())?;
                        let outputs = PaymentOutputs::from((script_p2sh, amount_sompi));
                        // TODO fee_rate
                        let fee_rate = None;
                        let priority_fee_sompi = try_parse_optional_kaspa_as_sompi_i64(argv.get(1))?.unwrap_or(0);
                        let abortable = Abortable::default();

                        let signer = account
                            .pskb_from_send_generator(
                                outputs.into(),
                                fee_rate,
                                priority_fee_sompi.into(),
                                None,
                                wallet_secret.clone(),
                                payment_secret.clone(),
                                &abortable,
                            )
                            .await?;

                        match signer.serialize() {
                            Ok(encoded) => tprintln!(ctx, "{encoded}"),
                            Err(e) => return Err(e.into()),
                        }
                    }
                    "unlock" => {
                        if argv.len() != 1 {
                            return self.display_help(ctx, argv).await;
                        }

                        // Get locked UTXO set.
                        let spend_utxos: Vec<kaspa_rpc_core::RpcUtxosByAddressesEntry> =
                            ctx.wallet().rpc_api().get_utxos_by_addresses(vec![script_p2sh.clone()]).await?;
                        let priority_fee_sompi = try_parse_optional_kaspa_as_sompi_i64(argv.first())?.unwrap_or(0) as u64;

                        if spend_utxos.is_empty() {
                            twarnln!(ctx, "No locked UTXO set found.");
                            return Ok(());
                        }

                        let references: Vec<(UtxoEntry, TransactionOutpoint)> =
                            spend_utxos.iter().map(|entry| (entry.utxo_entry.clone().into(), entry.outpoint.into())).collect();

                        let total_locked_sompi: u64 = spend_utxos.iter().map(|entry| entry.utxo_entry.amount).sum();

                        tprintln!(
                            ctx,
                            "{} locked UTXO{} found with total amount of {} KAS",
                            spend_utxos.len(),
                            if spend_utxos.len() == 1 { "" } else { "s" },
                            sompi_to_kaspa(total_locked_sompi)
                        );

                        // Sweep UTXO set.
                        match unlock_utxos_as_pskb(references, &receive_address, script_sig, priority_fee_sompi as u64) {
                            Ok(pskb) => {
                                let pskb_hex = pskb.serialize()?;
                                tprintln!(ctx, "{pskb_hex}");
                            }
                            Err(e) => tprintln!(ctx, "Error generating unlock PSKB: {}", e.to_string()),
                        }
                    }
                    "sign" => {
                        let pskb = Self::parse_input_pskb(argv.first().unwrap().as_str())?;

                        // Sign PSKB using the account's receiver address.
                        match account.pskb_sign(&pskb, wallet_secret.clone(), payment_secret.clone(), Some(&receive_address)).await {
                            Ok(signed_pskb) => {
                                let pskb_pack = String::try_from(signed_pskb)?;
                                tprintln!(ctx, "{pskb_pack}");
                            }
                            Err(e) => terrorln!(ctx, "{}", e.to_string()),
                        }
                    }
                    "address" => {
                        tprintln!(ctx, "\r\nP2SH address: {}", script_p2sh);
                    }
                    v => {
                        terrorln!(ctx, "unknown command: '{v}'\r\n");
                        return self.display_help(ctx, argv).await;
                    }
                }
            }
            "sign" => {
                if argv.len() != 1 {
                    return self.display_help(ctx, argv).await;
                }
                let (wallet_secret, payment_secret) = ctx.ask_wallet_secret(None).await?;
                let pskb = Self::parse_input_pskb(argv.first().unwrap().as_str())?;
                let account = ctx.wallet().account()?;
                match account.pskb_sign(&pskb, wallet_secret.clone(), payment_secret.clone(), None).await {
                    Ok(signed_pskb) => {
                        let pskb_pack = String::try_from(signed_pskb)?;
                        tprintln!(ctx, "{pskb_pack}");
                    }
                    Err(e) => terrorln!(ctx, "{}", e.to_string()),
                }
            }
            "sign-key" => {
                // Offline signing with a provided private key (32-byte hex). No wallet secrets required.
                // Usage: pskb sign-key <privkey-hex> <pskb>
                if argv.len() != 2 {
                    return self.display_help(ctx, argv).await;
                }
                let key_arg = argv.remove(0);
                let pskb = Self::parse_input_pskb(argv.first().unwrap().as_str())?;

                let privkey_bytes: [u8; 32] = {
                    if key_arg.len() == 64 && key_arg.chars().all(|c| c.is_ascii_hexdigit()) {
                        let mut buf = [0u8; 32];
                        faster_hex::hex_decode(key_arg.as_bytes(), &mut buf).map_err(|e| Error::custom(e.to_string()))?;
                        buf
                    } else {
                        return Err(Error::Custom("provide 32-byte hex private key".to_string()));
                    }
                };

                let kp = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, &privkey_bytes)
                    .map_err(|e| Error::custom(format!("invalid key: {e}")))?;

                let mut signed_bundle = Bundle::new();
                for inner in pskb.iter() {
                    let pskt: PSKT<Signer> = PSKT::from(inner.clone());
                    let signed = pskt.pass_signature_sync(|tx, sighash| {
                        let reused = kaspa_consensus_core::hashing::sighash::SigHashReusedValuesUnsync::new();
                        tx.tx
                            .inputs
                            .iter()
                            .enumerate()
                            .map(|(i, _)| {
                                let hash = kaspa_consensus_core::hashing::sighash::calc_schnorr_signature_hash(
                                    &tx.as_verifiable(),
                                    i,
                                    sighash[i],
                                    &reused,
                                );
                                let msg =
                                    secp256k1::Message::from_digest_slice(hash.as_bytes().as_slice()).map_err(|e| e.to_string())?;
                                let sig = kp.sign_schnorr(msg);
                                Ok(SignInputOk { signature: Signature::Schnorr(sig), pub_key: kp.public_key(), key_source: None })
                            })
                            .collect::<std::result::Result<Vec<SignInputOk>, String>>()
                    })?;
                    signed_bundle.add_pskt(signed);
                }

                let encoded = signed_bundle.serialize()?;
                tprintln!(ctx, "{encoded}");
            }
            "build" => {
                // Build a PSKB with one or more outputs.
                // Usage: pskb build [priority_fee] <address amount> [address amount]...
                if argv.len() < 2 {
                    return self.display_help(ctx, argv).await;
                }

                let mut args = argv;
                let mut priority_fee_sompi: i64 = 0;

                // Try to parse first arg as optional priority fee
                if let Ok(fee_opt) = try_parse_optional_kaspa_as_sompi_i64(Some(&args[0])) {
                    if let Some(fee_val) = fee_opt {
                        priority_fee_sompi = fee_val;
                        args.remove(0);
                    }
                }

                if args.len() % 2 != 0 {
                    return self.display_help(ctx, args).await;
                }

                let mut outputs_vec = Vec::new();
                while !args.is_empty() {
                    let addr = Address::try_from(args.remove(0).as_str())?;
                    let amt = try_parse_required_nonzero_kaspa_as_sompi_u64(Some(&args.remove(0)))?;
                    outputs_vec.push((addr, amt));
                }

                let outputs = PaymentOutputs::from(outputs_vec.as_slice());
                let (wallet_secret, payment_secret) = ctx.ask_wallet_secret(None).await?;
                let account: Arc<dyn Account> = ctx.wallet().account()?;
                let abortable = Abortable::default();

                let bundle = account
                    .pskb_from_send_generator(
                        outputs.into(),
                        None,
                        priority_fee_sompi.into(),
                        None,
                        wallet_secret.clone(),
                        payment_secret.clone(),
                        &abortable,
                    )
                    .await?;

                let encoded = bundle.serialize()?;
                tprintln!(ctx, "{encoded}");
            }
            "send" => {
                if argv.len() != 1 {
                    return self.display_help(ctx, argv).await;
                }
                let pskb = Self::parse_input_pskb(argv.first().unwrap().as_str())?;
                let account = ctx.wallet().account()?;
                match account.pskb_broadcast(&pskb).await {
                    Ok(sent) => tprintln!(ctx, "Sent transactions {:?}", sent),
                    Err(e) => terrorln!(ctx, "Send error {:?}", e),
                }
            }
            "redeem" => {
                // Print redeem script for the current multisig account (or provided address).
                // Usage: pskb redeem [address]
                let account = ctx.wallet().account()?;
                let multisig = account.clone().downcast_arc::<MultiSig>().map_err(|_| Error::InvalidAccountKind)?;

                let addr =
                    if argv.is_empty() { multisig.receive_address()? } else { Address::try_from(argv.first().unwrap().as_str())? };

                let script = multisig.redeem_script_for_address(&addr)?;
                let script_hex = hex_string(&script);
                tprintln!(ctx, "{script_hex}");
            }
            "debug" => {
                if argv.len() != 1 {
                    return self.display_help(ctx, argv).await;
                }
                let pskb = Self::parse_input_pskb(argv.first().unwrap().as_str())?;
                tprintln!(ctx, "{:?}", pskb);
            }
            "parse" => {
                if argv.len() != 1 {
                    return self.display_help(ctx, argv).await;
                }
                let pskb = Self::parse_input_pskb(argv.first().unwrap().as_str())?;
                tprintln!(ctx, "{}", pskb.display_format(ctx.wallet().network_id()?, sompi_to_kaspa_string_with_suffix));

                for (pskt_index, bundle_inner) in pskb.0.iter().enumerate() {
                    tprintln!(ctx, "PSKT #{:03} finalized check:", pskt_index + 1);
                    let pskt: PSKT<Signer> = PSKT::<Signer>::from(bundle_inner.to_owned());
                    let params = ctx.wallet().network_id()?.into();
                    let finalizer = pskt.finalizer();
                    if let Ok(pskt_finalizer) = finalize_pskt_one_or_more_sig_and_redeem_script(finalizer) {
                        // Verify if extraction is possible.
                        match pskt_finalizer.extractor() {
                            Ok(ex) => match ex.extract_tx(&params) {
                                Ok(_) => tprintln!(
                                    ctx,
                                    "  Transaction extracted successfully: PSKT is finalized with a valid script signature."
                                ),
                                Err(e) => terrorln!(ctx, "  PSKT transaction extraction error: {}", e.to_string()),
                            },
                            Err(_) => twarnln!(ctx, "  PSKT not finalized"),
                        }
                    } else {
                        twarnln!(ctx, "  PSKT not signed");
                    }
                }
            }
            v => {
                tprintln!(ctx, "unknown command: '{v}'\r\n");
                return self.display_help(ctx, argv).await;
            }
        }
        Ok(())
    }

    fn parse_input_pskb(input: &str) -> Result<Bundle> {
        match Bundle::try_from(input) {
            Ok(bundle) => Ok(bundle),
            Err(e) => Err(Error::custom(format!("Error while parsing input PSKB {}", e))),
        }
    }

    async fn display_help(self: Arc<Self>, ctx: Arc<KaspaCli>, _argv: Vec<String>) -> Result<()> {
        ctx.term().help(
            &[
                ("pskb create <address> <amount> <priority fee>", "Create a PSKB from single send transaction"),
                ("pskb sign <pskb>", "Sign given PSKB"),
                ("pskb sign-key <privkey-hex|wif> <pskb>", "Offline sign PSKB with provided private key"),
                ("pskb build [priority fee] <address amount> [address amount]...", "Build PSKB bundle with one or more outputs"),
                ("pskb send <pskb>", "Broadcast bundled transactions"),
                ("pskb debug <payload>", "Print PSKB debug view"),
                ("pskb parse <payload>", "Print PSKB formatted view"),
                ("pskb script lock <payload> <amount> [priority fee]", "Generate a PSKB with one send transaction to given P2SH payload. Optional public key placeholder in payload: {{pubkey}}"),
                ("pskb script unlock <payload> <fee>", "Generate a PSKB to unlock UTXOS one by one from given P2SH payload. Fee amount will be applied to every spent UTXO, meaning every transaction. Optional public key placeholder in payload: {{pubkey}}"),
                ("pskb script sign <pskb>", "Sign all PSKB's P2SH locked inputs"),
                ("pskb script sign <pskb>", "Sign all PSKB's P2SH locked inputs"),
                ("pskb script address <pskb>", "Prints P2SH address"),
                ("pskb redeem [address]", "Print redeem script for current multisig account (defaults to current receive)"),
            ],
            None,
        )?;

        Ok(())
    }
}
