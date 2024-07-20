use crate::imports::NetworkParams;
use crate::result::Result;
use crate::tx::mass;
use crate::wasm::tx::*;
use kaspa_consensus_client::*;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::tx as cctx;
use std::sync::Arc;
use wasm_bindgen::prelude::*;
use workflow_wasm::convert::*;

/// @category Wallet SDK
#[wasm_bindgen]
pub struct MassCalculator {
    mc: Arc<mass::MassCalculator>,
}

#[wasm_bindgen]
impl MassCalculator {
    #[wasm_bindgen(constructor)]
    pub fn new(cp: ConsensusParams) -> Self {
        let consensus_params = Params::from(cp);
        let network_params = NetworkParams::from(consensus_params.net);
        Self { mc: Arc::new(mass::MassCalculator::new(&consensus_params, &network_params)) }
    }

    #[wasm_bindgen(js_name=isDust)]
    pub fn is_dust(&self, amount: u64) -> bool {
        self.mc.is_dust(amount)
    }

    /// `isTransactionOutputDust()` returns whether or not the passed transaction output
    /// amount is considered dust or not based on the configured minimum transaction
    /// relay fee.
    ///
    /// Dust is defined in terms of the minimum transaction relay fee. In particular,
    /// if the cost to the network to spend coins is more than 1/3 of the minimum
    /// transaction relay fee, it is considered dust.
    ///
    /// It is exposed by `MiningManager` for use by transaction generators and wallets.
    #[wasm_bindgen(js_name=isTransactionOutputDust)]
    pub fn is_transaction_output_dust(transaction_output: &JsValue) -> Result<bool> {
        let transaction_output = TransactionOutput::try_from(transaction_output)?;
        let transaction_output = cctx::TransactionOutput::from(&transaction_output);
        Ok(mass::is_transaction_output_dust(&transaction_output))
    }

    /// `minimumRelayTransactionFee()` specifies the minimum transaction fee for a transaction to be accepted to
    /// the mempool and relayed. It is specified in sompi per 1kg (or 1000 grams) of transaction mass.
    ///
    /// `pub(crate) const MINIMUM_RELAY_TRANSACTION_FEE: u64 = 1000;`
    #[wasm_bindgen(js_name=minimumRelayTransactionFee)]
    pub fn minimum_relay_transaction_fee() -> u32 {
        mass::MINIMUM_RELAY_TRANSACTION_FEE as u32
    }

    /// `maximumStandardTransactionMass()` is the maximum mass allowed for transactions that
    /// are considered standard and will therefore be relayed and considered for mining.
    ///
    /// `pub const MAXIMUM_STANDARD_TRANSACTION_MASS: u64 = 100_000;`
    #[wasm_bindgen(js_name=maximumStandardTransactionMass)]
    pub fn maximum_standard_transaction_mass() -> u32 {
        mass::MAXIMUM_STANDARD_TRANSACTION_MASS as u32
    }

    /// minimum_required_transaction_relay_fee returns the minimum transaction fee required
    /// for a transaction with the passed mass to be accepted into the mempool and relayed.
    #[wasm_bindgen(js_name=minimumRequiredTransactionRelayFee)]
    pub fn calc_minimum_required_transaction_relay_fee(mass: u32) -> u32 {
        mass::calc_minimum_required_transaction_relay_fee(mass as u64) as u32
    }

    #[wasm_bindgen(js_name=calcMassForTransaction)]
    pub fn calc_mass_for_transaction(&self, tx: &JsValue) -> Result<u32> {
        let tx = Transaction::try_cast_from(tx)?;
        let tx = cctx::Transaction::from(tx.as_ref());
        Ok(self.mc.calc_mass_for_transaction(&tx) as u32)
    }

    #[wasm_bindgen(js_name=blankTransactionSerializedByteSize)]
    pub fn blank_transaction_serialized_byte_size() -> u32 {
        mass::blank_transaction_serialized_byte_size() as u32
    }

    #[wasm_bindgen(js_name=blankTransactionMass)]
    pub fn blank_transaction_mass(&self) -> u32 {
        self.mc.blank_transaction_mass() as u32
    }

    #[wasm_bindgen(js_name=calcMassForPayload)]
    pub fn calc_mass_for_payload(&self, payload_byte_size: usize) -> u32 {
        self.mc.calc_mass_for_payload(payload_byte_size) as u32
    }

    #[wasm_bindgen(js_name=calcMassForOutputs)]
    pub fn calc_mass_for_outputs(&self, outputs: JsValue) -> Result<u32> {
        let outputs = outputs
            .dyn_into::<js_sys::Array>()?
            .iter()
            .map(TransactionOutput::try_from)
            .collect::<std::result::Result<Vec<_>, kaspa_consensus_client::error::Error>>()?;
        let outputs = outputs.iter().map(|output| self.calc_mass_for_output(output)).collect::<Result<Vec<_>>>()?;
        Ok(outputs.iter().sum())
    }

    #[wasm_bindgen(js_name=calcMassForInputs)]
    pub fn calc_mass_for_inputs(&self, inputs: JsValue) -> Result<u32> {
        let inputs = inputs
            .dyn_into::<js_sys::Array>()?
            .iter()
            .map(TransactionInput::try_owned_from)
            .collect::<std::result::Result<Vec<_>, kaspa_consensus_client::error::Error>>()?;
        let inputs = inputs.iter().map(|input| self.calc_mass_for_input(input)).collect::<Result<Vec<_>>>()?;
        Ok(inputs.iter().sum())
    }

    #[wasm_bindgen(js_name=calcMassForOutput)]
    pub fn calc_mass_for_output(&self, output: &TransactionOutput) -> Result<u32> {
        // let output = TransactionOutput::try_from(output)?;
        let output = cctx::TransactionOutput::from(output);
        Ok(self.mc.calc_mass_for_output(&output) as u32)
    }

    #[wasm_bindgen(js_name=calcMassForInput)]
    pub fn calc_mass_for_input(&self, input: &TransactionInput) -> Result<u32> {
        // let input = TransactionInput::try_from(input)?;
        let input = cctx::TransactionInput::from(input);
        Ok(self.mc.calc_mass_for_input(&input) as u32)
    }

    #[wasm_bindgen(js_name=calcSignatureMass)]
    pub fn calc_signature_mass(&self, minimum_signatures: u16) -> u32 {
        self.mc.calc_signature_mass(minimum_signatures) as u32
    }

    #[wasm_bindgen(js_name=calcSignatureMassForInputs)]
    pub fn calc_signature_mass_for_inputs(&self, number_of_inputs: usize, minimum_signatures: u16) -> u32 {
        self.mc.calc_signature_mass_for_inputs(number_of_inputs, minimum_signatures) as u32
    }

    #[wasm_bindgen(js_name=calcMinimumTransactionRelayFeeFromMass)]
    pub fn calc_minimum_transaction_relay_fee_from_mass(&self, mass: u64) -> u32 {
        self.mc.calc_minimum_transaction_fee_from_mass(mass) as u32
    }

    #[wasm_bindgen(js_name=calcMiniumTxRelayFee)]
    pub fn calc_minimum_transaction_relay_fee(&self, transaction: &Transaction, minimum_signatures: u16) -> Result<u32> {
        let tx = cctx::Transaction::from(transaction);
        Ok(self.mc.calc_minium_transaction_relay_fee(&tx, minimum_signatures) as u32)
    }
}
