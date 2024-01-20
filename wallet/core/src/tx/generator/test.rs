#![allow(clippy::inconsistent_digit_grouping)]

use crate::error::Error;
use crate::result::Result;
use crate::tx::{Fees, MassCalculator, PaymentDestination};
use crate::utxo::UtxoEntryReference;
use crate::{tx::PaymentOutputs, utils::kaspa_to_sompi};
use kaspa_addresses::Address;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::tx::Transaction;
use std::cell::RefCell;
use std::fmt::Debug;
use std::rc::Rc;
use workflow_log::style;

use super::*;

const DISPLAY_LOGS: bool = true;
const DISPLAY_EXPECTED: bool = true;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Sompi(u64);

#[derive(Clone, Copy)]
struct Kaspa(f64);

impl Debug for Kaspa {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sompi: Sompi = self.into();
        write!(f, "{}", sompi.0)
    }
}

impl From<Kaspa> for Sompi {
    fn from(kaspa: Kaspa) -> Self {
        Sompi(kaspa_to_sompi(kaspa.0))
    }
}

impl From<&Kaspa> for Sompi {
    fn from(kaspa: &Kaspa) -> Self {
        Sompi(kaspa_to_sompi(kaspa.0))
    }
}

#[derive(Debug)]
enum FeesExpected {
    None,
    Sender(u64),
    Receiver(u64),
}

impl FeesExpected {
    fn sender<T: Into<Sompi>>(v: T) -> Self {
        let sompi: Sompi = v.into();
        FeesExpected::Sender(sompi.0)
    }
    fn receiver<T: Into<Sompi>>(v: T) -> Self {
        let sompi: Sompi = v.into();
        FeesExpected::Receiver(sompi.0)
    }
}

trait PendingTransactionExtension {
    fn tuple(self) -> (PendingTransaction, Transaction);
    fn expect<SOMPI>(self, expected: &Expected<SOMPI>) -> Self
    where
        SOMPI: Into<Sompi> + Debug + Copy;
    fn accumulate(self, accumulator: &mut Accumulator) -> Self;
}

impl PendingTransactionExtension for PendingTransaction {
    fn tuple(self) -> (PendingTransaction, Transaction) {
        let tx = self.transaction();
        (self, tx)
    }
    fn expect<SOMPI>(self, expected: &Expected<SOMPI>) -> Self
    where
        SOMPI: Into<Sompi> + Debug + Copy,
    {
        expect(&self, expected);
        self
    }
    fn accumulate(self, accumulator: &mut Accumulator) -> Self {
        accumulator.list.push(self.clone());
        self
    }
}

trait GeneratorSummaryExtension {
    fn check(self, accumulator: &Accumulator) -> Self;
}

impl GeneratorSummaryExtension for GeneratorSummary {
    fn check(self, accumulator: &Accumulator) -> Self {
        assert_eq!(self.number_of_generated_transactions, accumulator.list.len(), "number of generated transactions");
        assert_eq!(
            self.aggregated_utxos,
            accumulator.list.iter().map(|pt| pt.utxo_entries().len()).sum::<usize>(),
            "number of utxo entries"
        );
        let aggregated_fees = accumulator.list.iter().map(|pt| pt.fees()).sum::<u64>();
        assert_eq!(self.aggregated_fees, aggregated_fees, "aggregated fees");
        self
    }
}

trait FeesExtension {
    fn sender<T: Into<Sompi>>(v: T) -> Self;
    fn receiver<T: Into<Sompi>>(v: T) -> Self;
}

impl FeesExtension for Fees {
    fn sender<T: Into<Sompi>>(v: T) -> Self {
        let sompi: Sompi = v.into();
        Fees::SenderPays(sompi.0)
    }
    fn receiver<T: Into<Sompi>>(v: T) -> Self {
        let sompi: Sompi = v.into();
        Fees::ReceiverPays(sompi.0)
    }
}

trait GeneratorExtension {
    fn harness(self) -> Rc<Harness>;
}

impl GeneratorExtension for Generator {
    fn harness(self) -> Rc<Harness> {
        Harness::new(self)
    }
}

fn test_network_id() -> NetworkId {
    // TODO make this configurable
    NetworkId::with_suffix(NetworkType::Testnet, 11)
}

#[derive(Default)]
struct Accumulator {
    list: Vec<PendingTransaction>,
}

#[derive(Debug)]
pub(crate) struct Expected<SOMPI: Into<Sompi>> {
    is_final: bool,
    input_count: usize,
    aggregate_input_value: SOMPI,
    output_count: usize,
    priority_fees: FeesExpected,
}

fn expect<SOMPI>(pt: &PendingTransaction, expected: &Expected<SOMPI>)
where
    SOMPI: Into<Sompi> + Debug + Copy,
{
    let network_params = pt.generator().network_params();
    let tx = pt.transaction();

    let aggregate_input_value = pt.utxo_entries().iter().map(|o| o.amount()).sum::<u64>();
    let aggregate_output_value = tx.outputs.iter().map(|o| o.value).sum::<u64>();
    assert_ne!(aggregate_input_value, aggregate_output_value, "aggregate input and output values can not be the same due to fees");
    assert_eq!(pt.is_final(), expected.is_final, "expected final transaction");

    let expected_aggregate_input_value: Sompi = expected.aggregate_input_value.into();
    assert_eq!(tx.inputs.len(), expected.input_count, "expected input count");
    assert_eq!(aggregate_input_value, expected_aggregate_input_value.0, "expected aggregate input value");
    assert_eq!(tx.outputs.len(), expected.output_count, "expected output count");

    let pt_fees = pt.fees();
    let calc = MassCalculator::new(&pt.network_type().into(), network_params);
    let compute_mass = calc.calc_mass_for_signed_transaction(&tx, 1);

    let utxo_entries = pt.utxo_entries().iter().cloned().collect::<Vec<_>>();
    let storage_mass = calc.calc_storage_mass_for_transaction(false, &utxo_entries, &tx.outputs).unwrap_or_default();
    if DISPLAY_LOGS && storage_mass != 0 {
        println!(
            "calculated storage mass: {} calculated_compute_mass: {} total: {}",
            storage_mass,
            compute_mass,
            storage_mass + compute_mass
        );
    }

    let calculated_mass = calc.combine_mass(compute_mass, storage_mass);
    let calculated_fees = calc.calc_minimum_transaction_fee_from_mass(calculated_mass);

    if storage_mass != 0 {
        println!("PT outputs: {}", tx.outputs.len());
        println!("PT storage mass: {:?}", storage_mass);
    }

    assert_eq!(pt.inner.mass, calculated_mass, "pending transaction mass does not match calculated mass");

    match expected.priority_fees {
        FeesExpected::Sender(priority_fees) => {
            let total_fees_expected = priority_fees + calculated_fees;
            assert!(
                total_fees_expected <= pt_fees,
                "[Fees SENDER] total fees expected: {} are greater than the PT fees: {}",
                total_fees_expected,
                pt_fees
            );

            // test that fee difference is below dust value as this condition can
            // occur if a dust output has been consumed to fees, resulting in
            // mismatch between calculated fees and PT fees
            let dust_disposal_fees = pt_fees - total_fees_expected;
            if !calc.is_dust(dust_disposal_fees) {
                panic!("[Fees SENDER] dust_disposal_fees test failure - pt fees: {pt_fees}  expected fees: {total_fees_expected} difference: {dust_disposal_fees}");
            }

            assert_eq!(
                aggregate_input_value,
                aggregate_output_value + pt_fees,
                "aggregate input value vs total output value with fees"
            );
        }
        FeesExpected::Receiver(priority_fees) => {
            let total_fees_expected = priority_fees + calculated_fees;
            assert!(
                total_fees_expected <= pt_fees,
                "[Fees RECEIVER] total fees expected: {} is greater than PT fees: {}",
                total_fees_expected,
                pt_fees
            );

            // test that fee difference is below dust value as this condition can
            // occur if a dust output has been consumed to fees, resulting in
            // mismatch between calculated fees and PT fees
            let dust_disposal_fees = pt_fees - total_fees_expected;
            if !calc.is_dust(dust_disposal_fees) {
                panic!("[Fees RECEIVER] dust_disposal_fees test failure - pt fees: {pt_fees}  expected fees: {total_fees_expected} difference: {dust_disposal_fees}");
            }

            assert_eq!(
                aggregate_input_value - pt_fees,
                aggregate_output_value,
                "aggregate input value without fees vs total output value with fees"
            );
        }
        FeesExpected::None => {
            assert!(calculated_fees <= pt_fees, "total fees expected: {} is greater than PT fees: {}", calculated_fees, pt_fees);

            // test that fee difference is below dust value as this condition can
            // occur if a dust output has been consumed to fees, resulting in
            // mismatch between calculated fees and PT fees
            let dust_disposal_fees = pt_fees - calculated_fees;
            if !calc.is_dust(dust_disposal_fees) {
                panic!("[Fees NONE] dust_disposal_fees test failure - pt fees: {pt_fees}  calculated fees: {calculated_fees} difference: {dust_disposal_fees}");
            }

            let total_output_with_fees = aggregate_output_value + pt_fees;
            assert_eq!(aggregate_input_value, total_output_with_fees, "aggregate input value vs total output value with fees");
        }
    };
}

pub(crate) struct Harness {
    generator: Generator,
    accumulator: RefCell<Accumulator>,
}

impl Harness {
    pub fn new(generator: Generator) -> Rc<Self> {
        Rc::new(Harness { generator, accumulator: RefCell::new(Accumulator::default()) })
    }

    pub fn fetch<SOMPI: Into<Sompi>>(self: &Rc<Self>, expected: &Expected<SOMPI>) -> Rc<Self>
    where
        SOMPI: Into<Sompi> + Debug + Copy,
    {
        if DISPLAY_LOGS {
            println!("{}", style(format!("fetch - checking transaction: {}", self.accumulator.borrow().list.len())).magenta());

            if DISPLAY_EXPECTED {
                println!("{:#?}", expected);
            }
        }
        self.generator.generate_transaction().unwrap().unwrap().accumulate(&mut self.accumulator.borrow_mut()).expect(expected);
        self.clone()
    }

    pub fn drain<SOMPI>(self: &Rc<Self>, count: usize, expected: &Expected<SOMPI>) -> Rc<Self>
    where
        SOMPI: Into<Sompi> + Debug + Copy,
    {
        for _n in 0..count {
            if DISPLAY_LOGS {
                println!(
                    "{}",
                    style(format!("drain checking transaction: {} ({})", _n, self.accumulator.borrow().list.len())).magenta()
                );
            }
            self.generator.generate_transaction().unwrap().unwrap().accumulate(&mut self.accumulator.borrow_mut()).expect(expected);
        }
        self.clone()
    }

    pub fn finalize(self: Rc<Self>) {
        let pt = self.generator.generate_transaction().unwrap();
        assert!(pt.is_none(), "expected no more transactions");
        let summary = self.generator.summary();
        if DISPLAY_LOGS {
            println!("{:#?}", summary);
        }
        summary.check(&self.accumulator.borrow());
    }

    pub fn insufficient_funds(self: Rc<Self>) {
        match &self.generator.generate_transaction() {
            Ok(_pt) => {
                panic!("expected insufficient funds, instead received a transaction");
            }
            Err(err) => {
                assert!(matches!(&err, Error::InsufficientFunds { .. }), "expecting insufficient funds error, received: {:?}", err);
            }
        }
    }
}

pub(crate) fn generator<T, F>(network_id: NetworkId, head: &[f64], tail: &[f64], fees: Fees, outputs: &[(F, T)]) -> Result<Generator>
where
    T: Into<Sompi> + Clone,
    F: FnOnce(NetworkType) -> Address + Clone,
{
    let outputs = outputs
        .iter()
        .map(|(address, amount)| {
            let sompi: Sompi = (*amount).clone().into();
            (address.clone()(network_id.into()), sompi.0)
        })
        .collect::<Vec<_>>();
    make_generator(network_id, head, tail, fees, change_address, PaymentOutputs::from(outputs.as_slice()).into())
}

pub(crate) fn make_generator<F>(
    network_id: NetworkId,
    head: &[f64],
    tail: &[f64],
    fees: Fees,
    change_address: F,
    final_transaction_destination: PaymentDestination,
) -> Result<Generator>
where
    F: FnOnce(NetworkType) -> Address,
{
    let mut values = head.to_vec();
    values.extend(tail);

    let utxo_entries: Vec<UtxoEntryReference> = values.into_iter().map(kaspa_to_sompi).map(UtxoEntryReference::simulated).collect();
    let multiplexer = None;
    let sig_op_count = 1;
    let minimum_signatures = 1;
    let utxo_iterator: Box<dyn Iterator<Item = UtxoEntryReference> + Send + Sync + 'static> = Box::new(utxo_entries.into_iter());
    let source_utxo_context = None;
    let destination_utxo_context = None;
    let final_priority_fee = fees;
    let final_transaction_payload = None;
    let change_address = change_address(network_id.into());

    let settings = GeneratorSettings {
        network_id,
        multiplexer,
        sig_op_count,
        minimum_signatures,
        change_address,
        utxo_iterator,
        source_utxo_context,
        destination_utxo_context,
        final_transaction_priority_fee: final_priority_fee,
        final_transaction_destination,
        final_transaction_payload,
    };

    Generator::try_new(settings, None, None)
}

pub(crate) fn change_address(network_type: NetworkType) -> Address {
    match network_type {
        NetworkType::Mainnet => Address::try_from("kaspa:qpauqsvk7yf9unexwmxsnmg547mhyga37csh0kj53q6xxgl24ydxjsgzthw5j").unwrap(),
        NetworkType::Testnet => Address::try_from("kaspatest:qqz22l98sf8jun72rwh5rqe2tm8lhwtdxdmynrz4ypwak427qed5juktjt7ju").unwrap(),
        _ => unreachable!("network type not supported"),
    }
}

pub(crate) fn output_address(network_type: NetworkType) -> Address {
    match network_type {
        NetworkType::Mainnet => Address::try_from("kaspa:qrd9efkvg3pg34sgp6ztwyv3r569qlc43wa5w8nfs302532dzj47knu04aftm").unwrap(),
        NetworkType::Testnet => Address::try_from("kaspatest:qqrewmx4gpuekvk8grenkvj2hp7xt0c35rxgq383f6gy223c4ud5s58ptm6er").unwrap(),
        _ => unreachable!("network type not supported"),
    }
}

#[test]
fn test_generator_empty_utxo_noop() -> Result<()> {
    let generator = make_generator(test_network_id(), &[], &[], Fees::None, change_address, PaymentDestination::Change).unwrap();
    let tx = generator.generate_transaction().unwrap();
    assert!(tx.is_none());
    Ok(())
}

#[test]
fn test_generator_sweep_single_utxo_noop() -> Result<()> {
    let generator = make_generator(test_network_id(), &[10.0], &[], Fees::None, change_address, PaymentDestination::Change)
        .expect("single UTXO input: generator");
    let tx = generator.generate_transaction().unwrap();
    assert!(tx.is_none());
    Ok(())
}

#[test]
fn test_generator_sweep_two_utxos() -> Result<()> {
    make_generator(test_network_id(), &[10.0, 10.0], &[], Fees::None, change_address, PaymentDestination::Change)
        .expect("merge 2 UTXOs without fees: generator")
        .harness()
        .fetch(&Expected {
            is_final: true,
            input_count: 2,
            aggregate_input_value: Kaspa(20.0),
            output_count: 1,
            priority_fees: FeesExpected::None,
        })
        .finalize();
    Ok(())
}

#[test]
fn test_generator_sweep_two_utxos_with_priority_fees_rejection() -> Result<()> {
    let generator =
        make_generator(test_network_id(), &[10.0, 10.0], &[], Fees::sender(Kaspa(5.0)), change_address, PaymentDestination::Change);
    match generator {
        Err(Error::GeneratorFeesInSweepTransaction) => {}
        _ => panic!("merge 2 UTXOs with fees must fail generator creation"),
    }
    Ok(())
}

#[test]
fn test_generator_dust_1_1() -> Result<()> {
    generator(
        test_network_id(),
        &[10.0; 20],
        &[],
        Fees::sender(Kaspa(5.0)),
        [(output_address, Kaspa(1.0)), (output_address, Kaspa(1.0))].as_slice(),
    )
    .unwrap()
    .harness()
    .fetch(&Expected {
        is_final: true,
        input_count: 4,
        aggregate_input_value: Kaspa(40.0),
        output_count: 3,
        priority_fees: FeesExpected::sender(Kaspa(5.0)),
    })
    .finalize();

    Ok(())
}

// #[test]
// fn test_generator_dust_2() -> Result<()> {
//     generator(
//         test_network_id(),
//         &[10.0; 20],
//         &[],
//         Fees::sender(Kaspa(5.0)),
//         // [(output_address, Kaspa(0.125)), (output_address, Kaspa(1.0))].as_slice(),
//         [(output_address, Kaspa(0.1))].as_slice(),
//         // [(output_address, Kaspa(0.1))].as_slice(),
//     )
//     .unwrap()
//     .harness()
//     .fetch(&Expected {
//         is_final: true,
//         input_count: 2,
//         aggregate_input_value: Kaspa(20.0),
//         output_count: 3,
//         priority_fees: FeesExpected::sender(Kaspa(5.0)),
//     })
//     .finalize();

//     Ok(())
// }

#[test]
fn test_generator_inputs_2_outputs_2_fees_exclude() -> Result<()> {
    generator(
        test_network_id(),
        &[10.0; 2],
        &[],
        Fees::sender(Kaspa(5.0)),
        [(output_address, Kaspa(10.0)), (output_address, Kaspa(1.0))].as_slice(),
    )
    .unwrap()
    .harness()
    .fetch(&Expected {
        is_final: true,
        input_count: 2,
        aggregate_input_value: Kaspa(20.0),
        output_count: 3,
        priority_fees: FeesExpected::sender(Kaspa(5.0)),
    })
    .finalize();

    Ok(())
}

#[test]
fn test_generator_inputs_100_outputs_1_fees_exclude_success() -> Result<()> {
    // generator(test_network_id(), &[10.0; 100], &[], Fees::sender(Kaspa(5.0)), [(output_address, Kaspa(990.0))].as_slice())
    generator(test_network_id(), &[10.0; 100], &[], Fees::sender(Kaspa(0.0)), [(output_address, Kaspa(990.0))].as_slice())
        .unwrap()
        .harness()
        .fetch(&Expected {
            is_final: false,
            input_count: 88,
            aggregate_input_value: Kaspa(880.0),
            output_count: 1,
            priority_fees: FeesExpected::None,
        })
        .fetch(&Expected {
            is_final: false,
            input_count: 12,
            aggregate_input_value: Kaspa(120.0),
            output_count: 1,
            priority_fees: FeesExpected::None,
        })
        .fetch(&Expected {
            is_final: true,
            input_count: 2,
            aggregate_input_value: Sompi(999_99886776),
            output_count: 2,
            // priority_fees: FeesExpected::sender(Kaspa(5.0)),
            priority_fees: FeesExpected::sender(Kaspa(0.0)),
        })
        .finalize();

    Ok(())
}

#[test]
fn test_generator_inputs_100_outputs_1_fees_include_success() -> Result<()> {
    generator(
        test_network_id(),
        &[1.0; 100],
        &[],
        Fees::receiver(Kaspa(5.0)),
        // [(output_address, Kaspa(100.0))].as_slice(),
        [(output_address, Kaspa(100.0))].as_slice(),
    )
    .unwrap()
    .harness()
    .fetch(&Expected {
        is_final: false,
        input_count: 88,
        aggregate_input_value: Kaspa(88.0),
        output_count: 1,
        priority_fees: FeesExpected::None,
    })
    .fetch(&Expected {
        is_final: false,
        input_count: 12,
        aggregate_input_value: Kaspa(12.0),
        output_count: 1,
        priority_fees: FeesExpected::None,
    })
    .fetch(&Expected {
        is_final: true,
        input_count: 2,
        aggregate_input_value: Sompi(8799900698 + 1199986078),
        output_count: 1,
        priority_fees: FeesExpected::receiver(Kaspa(5.0)),
    })
    .finalize();

    Ok(())
}

#[test]
fn test_generator_inputs_100_outputs_1_fees_exclude_insufficient_funds() -> Result<()> {
    generator(test_network_id(), &[10.0; 100], &[], Fees::sender(Kaspa(5.0)), [(output_address, Kaspa(1000.0))].as_slice())
        .unwrap()
        .harness()
        .fetch(&Expected {
            is_final: false,
            input_count: 88,
            aggregate_input_value: Kaspa(880.0),
            output_count: 1,
            priority_fees: FeesExpected::None,
        })
        .insufficient_funds();

    Ok(())
}

#[test]
fn test_generator_inputs_903_outputs_2_fees_exclude() -> Result<()> {
    generator(test_network_id(), &[10.0; 1_000], &[], Fees::sender(Kaspa(5.0)), [(output_address, Kaspa(9_000.0))].as_slice())
        .unwrap()
        .harness()
        .drain(
            10,
            &Expected {
                is_final: false,
                input_count: 88,
                aggregate_input_value: Kaspa(880.0),
                output_count: 1,
                priority_fees: FeesExpected::None,
            },
        )
        .fetch(&Expected {
            is_final: false,
            input_count: 21,
            aggregate_input_value: Kaspa(210.0),
            output_count: 1,
            priority_fees: FeesExpected::None,
        })
        .fetch(&Expected {
            is_final: true,
            input_count: 11,
            aggregate_input_value: Sompi(9009_98982996),
            output_count: 2,
            priority_fees: FeesExpected::receiver(Kaspa(5.0)),
        })
        .finalize();

    Ok(())
}

// #[test]
// fn test_generator_1m_utxos_w_1kas_to_990k_sender_fees() -> Result<()> {
//     let harness = generator(
//         test_network_id(),
//         &[1.0; 1_000_000],
//         &[],
//         Fees::sender(Kaspa(5.0)),
//         [(output_address, Kaspa(990_000.0))].as_slice(),
//     )
//     .unwrap()
//     .harness();

//     harness
//         .drain(
//             1174,
//             &Expected {
//                 is_final: false,
//                 input_count: 843,
//                 aggregate_input_value: Kaspa(843.0).into(),
//                 output_count: 1,
//                 priority_fees: FeesExpected::None,
//             },
//         )
//         .fetch(&Expected {
//             is_final: false,
//             input_count: 325,
//             aggregate_input_value: Kaspa(325.0).into(),
//             output_count: 1,
//             priority_fees: FeesExpected::None,
//         })
//         .fetch(&Expected {
//             is_final: false,
//             input_count: 843,
//             aggregate_input_value: Sompi(710_648_15369544),
//             output_count: 1,
//             priority_fees: FeesExpected::None,
//         })
//         .fetch(&Expected {
//             is_final: false,
//             input_count: 332,
//             aggregate_input_value: Sompi(279_357_66731392),
//             output_count: 1,
//             priority_fees: FeesExpected::None,
//         })
//         .fetch(&Expected {
//             is_final: true,
//             input_count: 2,
//             aggregate_input_value: Sompi(990_005_81960862),
//             output_count: 2,
//             priority_fees: FeesExpected::sender(Kaspa(5.0)),
//         })
//         .finalize();

//     Ok(())
// }
