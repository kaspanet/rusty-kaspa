#![allow(unused_imports)]

use crate::imports::*;
use kaspa_addresses::Prefix;
use kaspa_consensus_core::hashing::sighash_type::{SIG_HASH_ALL, SigHashType};
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_wallet_core::account::pskb::finalize_pskt_one_or_more_sig_and_redeem_script;
use kaspa_wallet_pskt::{
    prelude::{Bundle, PSKT, Signer, lock_script_sig_templating, script_sig_to_address, unlock_utxos_as_pskb},
    pskt::Inner,
};
use std::collections::HashSet;

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
                        let allow_non_sighash_all = Self::take_allow_non_sighash_all_flag(&mut argv);
                        if argv.len() != 1 {
                            return self.display_help(ctx, argv).await;
                        }
                        let pskb = Self::parse_input_pskb(argv.first().unwrap().as_str())?;
                        Self::ensure_non_all_sighash_types_allowed(&pskb, allow_non_sighash_all)?;

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
                let allow_non_sighash_all = Self::take_allow_non_sighash_all_flag(&mut argv);
                if argv.len() != 1 {
                    return self.display_help(ctx, argv).await;
                }
                let pskb = Self::parse_input_pskb(argv.first().unwrap().as_str())?;
                Self::ensure_non_all_sighash_types_allowed(&pskb, allow_non_sighash_all)?;
                let (wallet_secret, payment_secret) = ctx.ask_wallet_secret(None).await?;
                let account = ctx.wallet().account()?;
                match account.pskb_sign(&pskb, wallet_secret.clone(), payment_secret.clone(), None).await {
                    Ok(signed_pskb) => {
                        let pskb_pack = String::try_from(signed_pskb)?;
                        tprintln!(ctx, "{pskb_pack}");
                    }
                    Err(e) => terrorln!(ctx, "{}", e.to_string()),
                }
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

    fn take_allow_non_sighash_all_flag(argv: &mut Vec<String>) -> bool {
        if let Some(index) = argv.iter().position(|argument| argument == "--allow-non-sighashall") {
            argv.remove(index);
            true
        } else {
            false
        }
    }

    fn non_all_sighash_types(pskb: &Bundle) -> HashSet<SigHashType> {
        pskb.0
            .iter()
            .flat_map(|inner| inner.inputs.iter().map(|input| input.sighash_type))
            .filter(|sighash_type| *sighash_type != SIG_HASH_ALL)
            .collect()
    }

    fn ensure_non_all_sighash_types_allowed(pskb: &Bundle, allow_non_sighash_all: bool) -> Result<()> {
        if allow_non_sighash_all {
            return Ok(());
        }

        let sighash_types = Self::non_all_sighash_types(pskb);
        if sighash_types.is_empty() {
            return Ok(());
        }

        let mut sighash_types = sighash_types.iter().map(ToString::to_string).collect::<Vec<_>>();
        sighash_types.sort();
        let sighash_types = sighash_types.join(", ");
        Err(Error::custom(format!(
            "Refusing to sign PSKB inputs using {sighash_types}. Pass --allow-non-sighashall to explicitly allow non-SIG_HASH_ALL signatures."
        )))
    }

    async fn display_help(self: Arc<Self>, ctx: Arc<KaspaCli>, _argv: Vec<String>) -> Result<()> {
        ctx.term().help(
            &[
                ("pskb create <address> <amount> <priority fee>", "Create a PSKB from single send transaction"),
                ("pskb sign <pskb> [--allow-non-sighashall]", "Sign given PSKB"),
                ("pskb send <pskb>", "Broadcast bundled transactions"),
                ("pskb debug <payload>", "Print PSKB debug view"),
                ("pskb parse <payload>", "Print PSKB formatted view"),
                ("pskb script lock <payload> <amount> [priority fee]", "Generate a PSKB with one send transaction to given P2SH payload. Optional public key placeholder in payload: {{pubkey}}"),
                ("pskb script unlock <payload> <fee>", "Generate a PSKB to unlock UTXOS one by one from given P2SH payload. Fee amount will be applied to every spent UTXO, meaning every transaction. Optional public key placeholder in payload: {{pubkey}}"),
                (
                    "pskb script sign <payload> <pskb> [--allow-non-sighashall]",
                    "Sign all PSKB's P2SH locked inputs",
                ),
                ("pskb script address <pskb>", "Prints P2SH address"),
            ],
            None,
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_NONE;
    use kaspa_wallet_pskt::prelude::InputBuilder;

    fn bundle_with_sighash_type(sighash_type: SigHashType) -> Bundle {
        let input = InputBuilder::default().sighash_type(sighash_type).build().unwrap();
        Bundle(vec![Inner { inputs: vec![input], ..Default::default() }])
    }

    #[test]
    fn non_all_sighash_type_requires_explicit_opt_in() {
        let non_all = bundle_with_sighash_type(SIG_HASH_NONE);
        let all = bundle_with_sighash_type(SIG_HASH_ALL);

        let error = Pskb::ensure_non_all_sighash_types_allowed(&non_all, false).unwrap_err();
        assert!(error.to_string().contains("--allow-non-sighashall"));
        assert!(Pskb::ensure_non_all_sighash_types_allowed(&non_all, true).is_ok());
        assert!(Pskb::ensure_non_all_sighash_types_allowed(&all, false).is_ok());
    }
}
