use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::mass::MassCalculator;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutpoint};
use kaspa_rpc_core::{RpcMempoolEntryByAddress, RpcUtxoEntry, RpcUtxosByAddressesEntry};
use kaspa_wallet_core::api::{TransactionOutpointWrapper, UtxoEntryWrapper};
use kaspa_wallet_core::prelude::{Address, PaymentDestination, PaymentOutput, PaymentOutputs};
use kaspa_wallet_core::tx::{Fees, Generator, GeneratorSettings, PendingTransaction};
use kaspa_wallet_core::utxo::{NetworkParams, UtxoEntryReference};
use kaspa_wallet_grpc_core::kaspawalletd::AddressBalances;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};
use tonic::Status;

pub const MIN_FEE_RATE: f64 = 100.0;
pub const USED_OUTPOINT_EXPIRY: Duration = Duration::from_secs(60);
const SIGNED_SCHNORR_SIGNATURE_SCRIPT_SIZE: usize = 66;

pub fn capped_fee_rate(transactions: impl IntoIterator<Item = (u64, u64)>, max_fee: u64) -> Option<f64> {
    transactions
        .into_iter()
        .filter(|(fees, _)| *fees > max_fee)
        .map(|(_, mass)| if mass == 0 { 0.0 } else { max_fee as f64 / mass as f64 })
        .reduce(f64::min)
}

pub fn fee_cap_error(max_fee: u64, capped_rate: f64) -> Status {
    Status::failed_precondition(format!(
        "setting max fee to {max_fee} results in a fee rate of {capped_rate}, which is below the minimum allowed fee rate of {MIN_FEE_RATE:.1} sompi/gram"
    ))
}

pub struct GenerationParams<'a> {
    pub network_id: NetworkId,
    pub utxos: Vec<UtxoEntryReference>,
    pub change_address: &'a Address,
    pub sig_op_count: u8,
    pub minimum_signatures: u16,
    pub to: &'a Address,
    pub amount: u64,
    pub is_send_all: bool,
}

pub fn generate(params: &GenerationParams<'_>, fee_rate: f64) -> Result<Vec<PendingTransaction>, Status> {
    let total = params.utxos.iter().fold(0u64, |total, utxo| total.saturating_add(utxo.utxo.amount));
    let (output_amount, priority_fee) =
        if params.is_send_all { (total, Fees::ReceiverPays(0)) } else { (params.amount, Fees::SenderPays(0)) };
    let settings = GeneratorSettings::try_new_with_iterator(
        params.network_id,
        Box::new(params.utxos.clone().into_iter()),
        None,
        params.change_address.clone(),
        params.sig_op_count,
        params.minimum_signatures,
        PaymentDestination::PaymentOutputs(PaymentOutputs { outputs: vec![PaymentOutput::new(params.to.clone(), output_amount)] }),
        Some(fee_rate),
        priority_fee,
        None,
        None,
    )
    .map_err(|err| Status::internal(err.to_string()))?;
    let generator = Generator::try_new(settings, None, None).map_err(|err| Status::internal(err.to_string()))?;
    generator.iter().collect::<Result<Vec<_>, _>>().map_err(|err| Status::internal(err.to_string()))
}

pub fn generate_with_max_fee(params: &GenerationParams<'_>, fee_rate: f64, max_fee: u64) -> Result<Vec<PendingTransaction>, Status> {
    let transactions = generate(params, fee_rate)?;
    let Some(capped_rate) = capped_fee_rate(transactions.iter().map(|tx| (tx.fees(), tx.mass())), max_fee) else {
        return Ok(transactions);
    };
    if capped_rate < MIN_FEE_RATE {
        return Err(fee_cap_error(max_fee, capped_rate));
    }
    let transactions = generate(params, capped_rate)?;
    if let Some(rate) = capped_fee_rate(transactions.iter().map(|tx| (tx.fees(), tx.mass())), max_fee) {
        return Err(fee_cap_error(max_fee, rate));
    }
    Ok(transactions)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeAddressSource {
    FirstFrom(Address),
    Current,
    New,
}

pub fn change_address_source(use_existing_change_address: bool, from_addresses: &[Address]) -> ChangeAddressSource {
    match (use_existing_change_address, from_addresses.first()) {
        (true, Some(address)) => ChangeAddressSource::FirstFrom(address.clone()),
        (true, None) => ChangeAddressSource::Current,
        (false, _) => ChangeAddressSource::New,
    }
}

#[derive(Default)]
pub struct UsedOutpoints {
    entries: HashMap<TransactionOutpoint, Instant>,
}

impl UsedOutpoints {
    pub fn record(&mut self, outpoints: impl IntoIterator<Item = TransactionOutpoint>, now: Instant) {
        for outpoint in outpoints {
            self.entries.insert(outpoint, now);
        }
    }

    pub fn prune(&mut self, now: Instant) {
        self.entries.retain(|_, recorded| now.saturating_duration_since(*recorded) < USED_OUTPOINT_EXPIRY);
    }

    pub fn is_used(&self, outpoint: &TransactionOutpoint, now: Instant) -> bool {
        self.entries.get(outpoint).is_some_and(|recorded| now.saturating_duration_since(*recorded) < USED_OUTPOINT_EXPIRY)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

pub fn coinbase_maturity(network_id: NetworkId) -> Result<u64, Status> {
    if network_id.network_type == NetworkType::Testnet && !matches!(network_id.suffix, Some(10) | Some(12)) {
        return Err(Status::internal(format!("unsupported network {network_id}")));
    }
    Ok(NetworkParams::from(network_id).coinbase_transaction_maturity_period_daa())
}

pub fn is_utxo_spendable(is_coinbase: bool, block_daa_score: u64, coinbase_maturity: u64, virtual_daa_score: u64) -> bool {
    !is_coinbase || block_daa_score.saturating_add(coinbase_maturity) < virtual_daa_score
}

pub fn mempool_spent_outpoints(entries: &[RpcMempoolEntryByAddress]) -> HashSet<TransactionOutpoint> {
    entries
        .iter()
        .flat_map(|entry| entry.sending.iter())
        .flat_map(|mempool_entry| mempool_entry.transaction.inputs.iter())
        .map(|input| TransactionOutpoint::from(input.previous_outpoint))
        .collect()
}

#[derive(Debug, Default, PartialEq)]
pub struct Balances {
    pub address_balances: Vec<AddressBalances>,
    pub available: u64,
    pub pending: u64,
}

pub fn compute_balances(
    entries: &[RpcUtxosByAddressesEntry],
    excluded: &HashSet<TransactionOutpoint>,
    virtual_daa_score: u64,
    coinbase_maturity: u64,
) -> Balances {
    let mut per_address: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for entry in entries {
        let Some(address) = entry.address.as_ref() else { continue };
        if excluded.contains(&TransactionOutpoint::from(entry.outpoint)) {
            continue;
        }
        let (available, pending) = per_address.entry(address.to_string()).or_default();
        let utxo = &entry.utxo_entry;
        if is_utxo_spendable(utxo.is_coinbase, utxo.block_daa_score, coinbase_maturity, virtual_daa_score) {
            *available = available.saturating_add(utxo.amount);
        } else {
            *pending = pending.saturating_add(utxo.amount);
        }
    }
    let mut balances = Balances::default();
    for (address, (available, pending)) in per_address {
        balances.available = balances.available.saturating_add(available);
        balances.pending = balances.pending.saturating_add(pending);
        balances.address_balances.push(AddressBalances { address, available, pending });
    }
    balances
}

pub fn signed_p2pk_input_compute_mass(params: &Params) -> u64 {
    let calculator = MassCalculator::new_with_consensus_params(params);
    let input = TransactionInput::new(TransactionOutpoint::default(), vec![0; SIGNED_SCHNORR_SIGNATURE_SCRIPT_SIZE], 0, 1);
    let with_input = Transaction::new(0, vec![input], vec![], 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
    let without_input = Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
    calculator
        .calc_non_contextual_masses(&with_input)
        .compute_mass
        .saturating_sub(calculator.calc_non_contextual_masses(&without_input).compute_mass)
}

pub fn fee_per_input(input_mass: u64, fee_rate: f64) -> u64 {
    (input_mass as f64 * fee_rate).ceil() as u64
}

pub fn is_external_utxo_spendable(entry: &RpcUtxoEntry, virtual_daa_score: u64, coinbase_maturity: u64, fee_per_input: u64) -> bool {
    if !entry.is_coinbase {
        return true;
    }
    if entry.amount <= fee_per_input {
        return false;
    }
    is_utxo_spendable(true, entry.block_daa_score, coinbase_maturity, virtual_daa_score)
}

pub fn utxo_entry_wrapper(entry: RpcUtxosByAddressesEntry) -> UtxoEntryWrapper {
    UtxoEntryWrapper {
        address: entry.address,
        outpoint: TransactionOutpointWrapper { transaction_id: entry.outpoint.transaction_id, index: entry.outpoint.index },
        amount: entry.utxo_entry.amount,
        script_public_key: entry.utxo_entry.script_public_key,
        block_daa_score: entry.utxo_entry.block_daa_score,
        is_coinbase: entry.utxo_entry.is_coinbase,
        covenant_id: entry.utxo_entry.covenant_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_addresses::{Prefix, Version};
    use kaspa_consensus_core::constants::SOMPI_PER_KASPA;
    use kaspa_consensus_core::tx::ScriptPublicKey;
    use kaspa_rpc_core::{RpcMempoolEntry, RpcTransaction, RpcTransactionId, RpcTransactionInput, RpcTransactionOutpoint};
    use tonic::Code;

    fn testnet() -> NetworkId {
        NetworkId::with_suffix(NetworkType::Testnet, 10)
    }

    fn address(byte: u8) -> Address {
        Address::new(Prefix::Testnet, Version::PubKey, &[byte; 32])
    }

    fn utxos(count: usize, amount: u64) -> Vec<UtxoEntryReference> {
        (0..count).map(|_| UtxoEntryReference::simulated_with_address(amount, &address(1))).collect()
    }

    fn params(
        utxos: Vec<UtxoEntryReference>,
        change: &Address,
        to: &Address,
        amount: u64,
        is_send_all: bool,
    ) -> GenerationParams<'static> {
        let change: &'static Address = Box::leak(Box::new(change.clone()));
        let to: &'static Address = Box::leak(Box::new(to.clone()));
        GenerationParams {
            network_id: testnet(),
            utxos,
            change_address: change,
            sig_op_count: 1,
            minimum_signatures: 1,
            to,
            amount,
            is_send_all,
        }
    }

    fn input_value(tx: &PendingTransaction) -> u64 {
        tx.utxo_entries().values().map(|entry| entry.utxo.amount).sum()
    }

    fn output_value(tx: &PendingTransaction) -> u64 {
        tx.transaction().outputs.iter().map(|output| output.value).sum()
    }

    fn assert_send_all(transactions: &[PendingTransaction], total: u64, recipient: &Address) {
        let final_tx = transactions.last().unwrap();
        assert!(final_tx.is_final());
        assert!(transactions.iter().rev().skip(1).all(|tx| tx.is_batch()));
        let final_outputs = final_tx.transaction().outputs;
        assert_eq!(final_outputs.len(), 1);
        assert_eq!(final_outputs[0].script_public_key, kaspa_txscript::pay_to_address_script(recipient));
        assert!(final_tx.change_output_index().is_none());
        assert_eq!(final_outputs[0].value.checked_add(final_tx.fees()).unwrap(), input_value(final_tx));
        let aggregate_fees: u64 = transactions.iter().map(|tx| tx.fees()).sum();
        assert_eq!(final_outputs[0].value, total.checked_sub(aggregate_fees).unwrap());
        for tx in transactions {
            assert_eq!(output_value(tx).checked_add(tx.fees()).unwrap(), input_value(tx));
        }
    }

    #[test]
    fn send_all_single_transaction() {
        let total = 3u64.checked_mul(10 * SOMPI_PER_KASPA).unwrap();
        let transactions = generate(&params(utxos(3, 10 * SOMPI_PER_KASPA), &address(2), &address(3), 0, true), 1000.0).unwrap();
        assert_eq!(transactions.len(), 1);
        assert_send_all(&transactions, total, &address(3));
    }

    #[test]
    fn send_all_chained_transactions() {
        let total = 1000u64.checked_mul(SOMPI_PER_KASPA).unwrap();
        let transactions = generate(&params(utxos(1000, SOMPI_PER_KASPA), &address(2), &address(3), 0, true), 1000.0).unwrap();
        assert!(transactions.len() > 1);
        assert_send_all(&transactions, total, &address(3));
    }

    #[test]
    fn send_amount_keeps_change() {
        let transactions =
            generate(&params(utxos(3, 10 * SOMPI_PER_KASPA), &address(2), &address(3), 5 * SOMPI_PER_KASPA, false), 1000.0).unwrap();
        let final_tx = transactions.last().unwrap();
        let outputs = final_tx.transaction().outputs;
        assert_eq!(outputs[0].value, 5 * SOMPI_PER_KASPA);
        assert!(final_tx.change_output_index().is_some());
    }

    #[test]
    fn max_fee_caps_fee_rate() {
        let generation = params(utxos(3, 10 * SOMPI_PER_KASPA), &address(2), &address(3), 5 * SOMPI_PER_KASPA, false);
        let uncapped = generate(&generation, 1000.0).unwrap();
        let uncapped_fee = uncapped.iter().map(|tx| tx.fees()).max().unwrap();
        let max_fee = uncapped_fee / 4;
        let capped = generate_with_max_fee(&generation, 1000.0, max_fee).unwrap();
        assert!(capped.iter().all(|tx| tx.fees() <= max_fee));
        assert!(capped.iter().map(|tx| tx.fees()).max().unwrap() < uncapped_fee);
    }

    #[test]
    fn max_fee_not_reached_keeps_rate() {
        let generation = params(utxos(3, 10 * SOMPI_PER_KASPA), &address(2), &address(3), 5 * SOMPI_PER_KASPA, false);
        let expected = generate(&generation, 1000.0).unwrap().iter().map(|tx| tx.fees()).collect::<Vec<_>>();
        let actual =
            generate_with_max_fee(&generation, 1000.0, SOMPI_PER_KASPA).unwrap().iter().map(|tx| tx.fees()).collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }

    #[test]
    fn max_fee_below_minimum_rate_is_rejected() {
        let generation = params(utxos(3, 10 * SOMPI_PER_KASPA), &address(2), &address(3), 5 * SOMPI_PER_KASPA, false);
        let err = generate_with_max_fee(&generation, 1000.0, 1000).unwrap_err();
        assert_eq!(err.code(), Code::FailedPrecondition);
        assert!(err.message().starts_with("setting max fee to 1000 results in a fee rate of "));
        assert!(err.message().ends_with("which is below the minimum allowed fee rate of 100.0 sompi/gram"));
    }

    #[test]
    fn capped_fee_rate_uses_minimum_over_offending() {
        assert_eq!(capped_fee_rate([(100, 10), (50, 10)], 200), None);
        assert_eq!(capped_fee_rate([(300, 10), (500, 4), (100, 1)], 200), Some(20.0));
        assert_eq!(capped_fee_rate([(300, 0)], 200), Some(0.0));
        assert_eq!(capped_fee_rate(std::iter::empty(), 200), None);
    }

    #[test]
    fn change_address_selection() {
        let from = vec![address(5), address(6)];
        assert_eq!(change_address_source(true, &from), ChangeAddressSource::FirstFrom(address(5)));
        assert_eq!(change_address_source(true, &[]), ChangeAddressSource::Current);
        assert_eq!(change_address_source(false, &from), ChangeAddressSource::New);
        assert_eq!(change_address_source(false, &[]), ChangeAddressSource::New);
    }

    fn outpoint(byte: u8, index: u32) -> TransactionOutpoint {
        TransactionOutpoint::new(RpcTransactionId::from_bytes([byte; 32]), index)
    }

    #[test]
    fn used_outpoints_expire() {
        let start = Instant::now();
        let mut used = UsedOutpoints::default();
        used.record([outpoint(1, 0), outpoint(2, 1)], start);
        assert!(used.is_used(&outpoint(1, 0), start));
        assert!(!used.is_used(&outpoint(3, 0), start));
        let almost = start.checked_add(Duration::from_secs(59)).unwrap();
        assert!(used.is_used(&outpoint(2, 1), almost));
        used.prune(almost);
        assert_eq!(used.len(), 2);
        let expired = start.checked_add(USED_OUTPOINT_EXPIRY).unwrap();
        assert!(!used.is_used(&outpoint(1, 0), expired));
        used.record([outpoint(3, 0)], almost);
        used.prune(expired);
        assert_eq!(used.len(), 1);
        assert!(used.is_used(&outpoint(3, 0), expired));
        used.prune(start.checked_add(Duration::from_secs(200)).unwrap());
        assert!(used.is_empty());
    }

    fn node_entry(
        address_byte: Option<u8>,
        outpoint_byte: u8,
        amount: u64,
        is_coinbase: bool,
        block_daa_score: u64,
    ) -> RpcUtxosByAddressesEntry {
        RpcUtxosByAddressesEntry {
            address: address_byte.map(address),
            outpoint: RpcTransactionOutpoint { transaction_id: RpcTransactionId::from_bytes([outpoint_byte; 32]), index: 0 },
            utxo_entry: RpcUtxoEntry {
                amount,
                script_public_key: ScriptPublicKey::from_vec(0, vec![]),
                block_daa_score,
                is_coinbase,
                covenant_id: None,
            },
        }
    }

    #[test]
    fn coinbase_maturity_per_network() {
        assert_eq!(coinbase_maturity(testnet()).unwrap(), NetworkParams::from(testnet()).coinbase_transaction_maturity_period_daa());
        assert!(coinbase_maturity(NetworkId::new(NetworkType::Mainnet)).is_ok());
        assert!(coinbase_maturity(NetworkId::with_suffix(NetworkType::Testnet, 12)).is_ok());
        assert_eq!(coinbase_maturity(NetworkId::with_suffix(NetworkType::Testnet, 11)).unwrap_err().code(), Code::Internal);
        assert_eq!(
            coinbase_maturity(NetworkId { network_type: NetworkType::Testnet, suffix: None }).unwrap_err().code(),
            Code::Internal
        );
    }

    #[test]
    fn utxo_spendability() {
        assert!(is_utxo_spendable(false, 1000, 100, 0));
        assert!(is_utxo_spendable(true, 1000, 100, 1101));
        assert!(!is_utxo_spendable(true, 1000, 100, 1100));
        assert!(!is_utxo_spendable(true, u64::MAX, 100, u64::MAX));
    }

    #[test]
    fn balances_per_address() {
        let entries = vec![
            node_entry(Some(1), 1, 100, false, 0),
            node_entry(Some(1), 2, 200, true, 1000),
            node_entry(Some(1), 3, 300, true, 500),
            node_entry(Some(2), 4, 400, false, 0),
            node_entry(Some(2), 5, 500, false, 0),
            node_entry(Some(3), 6, 600, false, 0),
            node_entry(None, 7, 700, false, 0),
        ];
        let excluded = HashSet::from([outpoint(5, 0), outpoint(6, 0)]);
        let balances = compute_balances(&entries, &excluded, 1050, 100);
        assert_eq!(
            balances.address_balances,
            vec![
                AddressBalances { address: address(2).to_string(), available: 400, pending: 0 },
                AddressBalances { address: address(1).to_string(), available: 400, pending: 200 },
            ]
        );
        assert_eq!(balances.available, 800);
        assert_eq!(balances.pending, 200);
        assert_eq!(balances.available, balances.address_balances.iter().map(|b| b.available).sum::<u64>());
        assert_eq!(balances.pending, balances.address_balances.iter().map(|b| b.pending).sum::<u64>());
        assert_eq!(compute_balances(&[], &HashSet::new(), 0, 100), Balances::default());
    }

    fn mempool_tx(outpoints: &[(u8, u32)]) -> RpcMempoolEntry {
        RpcMempoolEntry {
            fee: 0,
            is_orphan: false,
            transaction: RpcTransaction {
                version: 0,
                inputs: outpoints
                    .iter()
                    .map(|(byte, index)| RpcTransactionInput {
                        previous_outpoint: RpcTransactionOutpoint {
                            transaction_id: RpcTransactionId::from_bytes([*byte; 32]),
                            index: *index,
                        },
                        signature_script: vec![],
                        sequence: 0,
                        sig_op_count: 1,
                        compute_budget: 0,
                        verbose_data: None,
                    })
                    .collect(),
                outputs: vec![],
                lock_time: 0,
                subnetwork_id: SUBNETWORK_ID_NATIVE,
                gas: 0,
                payload: vec![],
                storage_mass: 0,
                verbose_data: None,
            },
        }
    }

    #[test]
    fn mempool_sending_outpoints_are_collected() {
        let entries = vec![
            RpcMempoolEntryByAddress {
                address: address(1),
                sending: vec![mempool_tx(&[(1, 0), (2, 3)])],
                receiving: vec![mempool_tx(&[(9, 9)])],
            },
            RpcMempoolEntryByAddress { address: address(2), sending: vec![mempool_tx(&[(4, 1)])], receiving: vec![] },
        ];
        assert_eq!(mempool_spent_outpoints(&entries), HashSet::from([outpoint(1, 0), outpoint(2, 3), outpoint(4, 1)]));
    }

    #[test]
    fn signed_input_mass() {
        let mass = signed_p2pk_input_compute_mass(&Params::from(NetworkType::Testnet));
        assert_eq!(mass, 1118);
        assert_eq!(signed_p2pk_input_compute_mass(&Params::from(NetworkType::Mainnet)), mass);
        assert_eq!(fee_per_input(mass, 100.0), 111_800);
        assert_eq!(fee_per_input(mass, 1.5), 1677);
        assert_eq!(fee_per_input(3, 0.5), 2);
    }

    #[test]
    fn external_utxo_filter() {
        let entry = |amount, is_coinbase, block_daa_score| node_entry(Some(1), 1, amount, is_coinbase, block_daa_score).utxo_entry;
        assert!(is_external_utxo_spendable(&entry(1, false, 0), 0, 100, 1000));
        assert!(is_external_utxo_spendable(&entry(5000, true, 100), 201, 100, 1000));
        assert!(!is_external_utxo_spendable(&entry(1000, true, 100), 201, 100, 1000));
        assert!(!is_external_utxo_spendable(&entry(5000, true, 100), 200, 100, 1000));
    }

    #[test]
    fn utxo_entry_wrapper_mapping() {
        let wrapper = utxo_entry_wrapper(node_entry(Some(1), 7, 4242, true, 99));
        assert_eq!(wrapper.address, Some(address(1)));
        assert_eq!(wrapper.outpoint.transaction_id, RpcTransactionId::from_bytes([7; 32]));
        assert_eq!(wrapper.outpoint.index, 0);
        assert_eq!(wrapper.amount, 4242);
        assert_eq!(wrapper.block_daa_score, 99);
        assert!(wrapper.is_coinbase);
    }
}
