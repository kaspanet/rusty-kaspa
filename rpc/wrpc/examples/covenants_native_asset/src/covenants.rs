use kaspa_consensus_core::hashing::tx::transaction_id_preimage;
use kaspa_consensus_core::mass::{MassCalculator, NonContextualMasses};
use kaspa_consensus_core::subnets::SubnetworkId;
use kaspa_consensus_core::tx::{
    PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
};
use kaspa_hashes::{Hasher, TransactionID};
use kaspa_txscript::opcodes::codes::{
    Op1Sub, Op2Drop, Op2Dup, OpAdd, OpBin2Num, OpBlake2bWithKey, OpCat, OpData1, OpDrop, OpDup, OpElse, OpEndIf, OpEqual,
    OpEqualVerify, OpGreaterThanOrEqual, OpIf, OpNumEqualVerify, OpOutpointIndex, OpOutpointTxId, OpPick, OpSize, OpSub, OpSubStr,
    OpSwap, OpTrue, OpTuck, OpTxInputCount, OpTxInputIndex, OpTxInputSpk, OpTxOutputCount, OpTxOutputSpk, OpTxPayloadLen,
    OpTxPayloadSubstr, OpVerify, OpWithin,
};
use kaspa_txscript::script_builder::{ScriptBuilder, ScriptBuilderError};
use kaspa_txscript::SpkEncoding;
use std::convert::TryInto;

pub use crate::errors::CovenantError;
use crate::payload_layout::{
    MintPayloadLayout, PayloadHeader, TransferPayloadLayout, ASSET_ID_SIZE, MAX_INPUTS_COUNT, MAX_OUTPUTS_COUNT, MINT_PAYLOAD_LEN,
    PAYLOAD_MAGIC, SPK_BYTES_MAX, SPK_BYTES_MIN, TRANSFER_PAYLOAD_LEN,
};
use crate::result::CovenantResult;
use crate::scriptnum::{append_u64_le, decode_u64_le};

// pre-image bytes layout
const TX_VERSION_SIZE: usize = 2;
const U64_SIZE: usize = 8;
const OUTPOINT_TXID_SIZE: usize = 32;
const OUTPOINT_INDEX_SIZE: usize = 4;
const INPUT_NO_SIG_SCRIPT_SIZE: usize = OUTPOINT_TXID_SIZE + OUTPOINT_INDEX_SIZE + U64_SIZE + U64_SIZE;
const OUTPUT_VALUE_SIZE: usize = 8;
const SPK_VERSION_SIZE: usize = 2;
const SPK_SCRIPT_LEN_SIZE: usize = U64_SIZE;
// Offsets into transaction_id_preimage() for parent input0 prevout fields.
// Layout is: tx_version (2) + input_count (u64) + input0.prevout(txid + index) + ...
// Used by the covenant to extract the parent input0 prevout for asset_id derivation and GP binding.
const PARENT_INPUT0_PREVOUT_TXID_START: usize = TX_VERSION_SIZE + U64_SIZE;
const PARENT_INPUT0_PREVOUT_TXID_END: usize = PARENT_INPUT0_PREVOUT_TXID_START + OUTPOINT_TXID_SIZE;
const PARENT_INPUT0_PREVOUT_INDEX_START: usize = PARENT_INPUT0_PREVOUT_TXID_END;
const PARENT_INPUT0_PREVOUT_INDEX_END: usize = PARENT_INPUT0_PREVOUT_INDEX_START + OUTPOINT_INDEX_SIZE;

const TOKEN_OP_MINT: u8 = 0;
const TOKEN_OP_SPLIT_MERGE: u8 = 2;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeAssetOp {
    Mint = TOKEN_OP_MINT,
    SplitMerge = TOKEN_OP_SPLIT_MERGE,
}

impl NativeAssetOp {
    fn from_byte(value: u8) -> Option<Self> {
        match value {
            TOKEN_OP_MINT => Some(Self::Mint),
            TOKEN_OP_SPLIT_MERGE => Some(Self::SplitMerge),
            _ => None,
        }
    }
}

/// KNAT20 payload encoded as fixed offsets with separate mint and transfer layouts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeAssetPayload {
    /// outpoint_txid || outpoint_index_le of the mint's input 0.
    pub asset_id: [u8; ASSET_ID_SIZE],
    /// spk.to_bytes() (version + script)
    pub authority_spk_bytes: Vec<u8>,
    /// spk.to_bytes() (version + script)
    pub token_spk_bytes: Vec<u8>,
    pub remaining_supply: u64,
    pub op: NativeAssetOp,
    // total requested amount =sum_of(outputs[n].amount)
    pub total_amount: u64,
    // amounts in each inputs (by index)
    pub input_amounts: Vec<u64>,
    pub outputs: Vec<NativeAssetOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeAssetOutput {
    pub amount: u64,
    /// spk.to_bytes() (version + script)
    pub recipient_spk_bytes: Vec<u8>,
}

impl NativeAssetPayload {
    /// Returns the new payload after minting.
    /// Errors if the amount exceeds the remaining supply.
    pub fn mint_next(&self, amount: u64, recipient_spk_bytes: &[u8]) -> Result<Self, CovenantError> {
        validate_spk_bytes(recipient_spk_bytes, "recipient_spk_bytes")?;
        let remaining = self
            .remaining_supply
            .checked_sub(amount)
            .ok_or(CovenantError::AmountExceedsRemainingSupply { remaining: self.remaining_supply, amount })?;

        Ok(Self {
            asset_id: self.asset_id,
            authority_spk_bytes: self.authority_spk_bytes.clone(),
            token_spk_bytes: self.token_spk_bytes.clone(),
            remaining_supply: remaining,
            op: NativeAssetOp::Mint,
            total_amount: amount,
            input_amounts: Vec::new(),
            outputs: vec![NativeAssetOutput { amount, recipient_spk_bytes: recipient_spk_bytes.to_vec() }],
        })
    }

    /// Returns the new payload after transferring.
    pub fn token_transfer_next(&self, new_recipient_spk_bytes: &[u8]) -> Result<Self, CovenantError> {
        let output = NativeAssetOutput { amount: self.total_amount, recipient_spk_bytes: new_recipient_spk_bytes.to_vec() };
        self.split_merge_next(&[self.total_amount], &[output])
    }

    /// Returns the new payload after split/merge.
    pub fn split_merge_next(&self, input_amounts: &[u64], outputs: &[NativeAssetOutput]) -> Result<Self, CovenantError> {
        if input_amounts.len() > MAX_INPUTS_COUNT {
            return Err(CovenantError::InvalidField("input_amounts"));
        }
        if outputs.len() > MAX_OUTPUTS_COUNT {
            return Err(CovenantError::InvalidField("outputs"));
        }
        if input_amounts.is_empty() {
            return Err(CovenantError::InvalidField("input_count"));
        }
        if outputs.is_empty() {
            return Err(CovenantError::InvalidField("output_count"));
        }
        for output in outputs {
            validate_spk_bytes(&output.recipient_spk_bytes, "recipient_spk_bytes")?;
        }

        let total_in: u64 = input_amounts.iter().sum();
        let total_out: u64 = outputs.iter().map(|output| output.amount).sum();
        if total_in != total_out {
            return Err(CovenantError::InvalidField("amounts"));
        }

        Ok(Self {
            asset_id: self.asset_id,
            authority_spk_bytes: self.authority_spk_bytes.clone(),
            token_spk_bytes: self.token_spk_bytes.clone(),
            remaining_supply: self.remaining_supply,
            op: NativeAssetOp::SplitMerge,
            total_amount: total_out,
            input_amounts: input_amounts.to_vec(),
            outputs: outputs.to_vec(),
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, CovenantError> {
        validate_spk_bytes(&self.authority_spk_bytes, "authority_spk_bytes")?;
        validate_spk_bytes(&self.token_spk_bytes, "token_spk_bytes")?;

        let input_count = self.input_amounts.len();
        let output_count = self.outputs.len();
        if input_count > MAX_INPUTS_COUNT {
            return Err(CovenantError::InvalidField("input_amounts"));
        }
        if output_count > MAX_OUTPUTS_COUNT {
            return Err(CovenantError::InvalidField("outputs"));
        }

        for &amount in &self.input_amounts {
            if amount == 0 {
                return Err(CovenantError::InvalidField("input_amount"));
            }
        }
        for output in &self.outputs {
            if output.amount == 0 {
                return Err(CovenantError::InvalidField("output_amount"));
            }
            validate_spk_bytes(&output.recipient_spk_bytes, "recipient_spk_bytes")?;
        }

        let total_inputs: u64 = self.input_amounts.iter().sum();
        let total_outputs: u64 = self.outputs.iter().map(|output| output.amount).sum();
        match self.op {
            NativeAssetOp::Mint => {
                if input_count != 0 {
                    return Err(CovenantError::InvalidField("input_count"));
                }
                if output_count != 1 {
                    return Err(CovenantError::InvalidField("output_count"));
                }
                if total_outputs != self.total_amount {
                    return Err(CovenantError::InvalidField("total_amount"));
                }

                let output = self.outputs.get(0).ok_or(CovenantError::InvalidField("output_count"))?;

                let mut payload = Vec::with_capacity(MINT_PAYLOAD_LEN);
                payload.extend_from_slice(PAYLOAD_MAGIC);
                payload.extend_from_slice(&self.asset_id);
                append_spk_bytes(&mut payload, &self.authority_spk_bytes, "authority_spk_bytes")?;
                append_spk_bytes(&mut payload, &self.token_spk_bytes, "token_spk_bytes")?;
                append_u64_le(&mut payload, self.remaining_supply, "remaining_supply")?;
                payload.push(self.op as u8);
                append_u64_le(&mut payload, self.total_amount, "total_amount")?;
                append_u64_le(&mut payload, output.amount, "output_amount")?;
                append_spk_bytes(&mut payload, &output.recipient_spk_bytes, "recipient_spk_bytes")?;
                Ok(payload)
            }
            NativeAssetOp::SplitMerge => {
                if input_count == 0 {
                    return Err(CovenantError::InvalidField("input_count"));
                }
                if output_count == 0 {
                    return Err(CovenantError::InvalidField("output_count"));
                }
                if total_inputs != self.total_amount || total_outputs != self.total_amount {
                    return Err(CovenantError::InvalidField("total_amount"));
                }

                let mut payload = Vec::with_capacity(TRANSFER_PAYLOAD_LEN);
                payload.extend_from_slice(PAYLOAD_MAGIC);
                payload.extend_from_slice(&self.asset_id);
                append_spk_bytes(&mut payload, &self.authority_spk_bytes, "authority_spk_bytes")?;
                append_spk_bytes(&mut payload, &self.token_spk_bytes, "token_spk_bytes")?;
                append_u64_le(&mut payload, self.remaining_supply, "remaining_supply")?;
                payload.push(self.op as u8);
                append_u64_le(&mut payload, self.total_amount, "total_amount")?;

                for index in 0..MAX_INPUTS_COUNT {
                    let amount = self.input_amounts.get(index).copied().unwrap_or(0);
                    append_u64_le(&mut payload, amount, "input_amount")?;
                }
                for index in 0..MAX_OUTPUTS_COUNT {
                    let amount = self.outputs.get(index).map(|output| output.amount).unwrap_or(0);
                    append_u64_le(&mut payload, amount, "output_amount")?;
                }
                for index in 0..MAX_OUTPUTS_COUNT {
                    if let Some(output) = self.outputs.get(index) {
                        append_spk_bytes(&mut payload, &output.recipient_spk_bytes, "recipient_spk_bytes")?;
                    } else {
                        append_spk_bytes_padding(&mut payload);
                    }
                }
                Ok(payload)
            }
        }
    }

    pub fn decode(payload: &[u8]) -> Result<Self, CovenantError> {
        let is_mint_payload = payload.len() == MINT_PAYLOAD_LEN;
        let is_transfer_payload = payload.len() == TRANSFER_PAYLOAD_LEN;
        if !is_mint_payload && !is_transfer_payload {
            return Err(CovenantError::InvalidPayloadLength { expected: TRANSFER_PAYLOAD_LEN, actual: payload.len() });
        }
        if &payload[PayloadHeader::MAGIC.start..PayloadHeader::MAGIC.end] != PAYLOAD_MAGIC {
            return Err(CovenantError::InvalidPayloadMagic);
        }

        let asset_id = payload[PayloadHeader::ASSET_ID.start..PayloadHeader::ASSET_ID.end]
            .try_into()
            .map_err(|_| CovenantError::InvalidField("asset_id"))?;
        let authority_spk_bytes = decode_spk_bytes(
            payload,
            PayloadHeader::AUTHORITY_SPK.len.start,
            PayloadHeader::AUTHORITY_SPK.bytes.start,
            PayloadHeader::AUTHORITY_SPK.bytes.end,
            "authority_spk_bytes",
        )?;
        let token_spk_bytes = decode_spk_bytes(
            payload,
            PayloadHeader::TOKEN_SPK.len.start,
            PayloadHeader::TOKEN_SPK.bytes.start,
            PayloadHeader::TOKEN_SPK.bytes.end,
            "token_spk_bytes",
        )?;
        let remaining_supply =
            decode_u64_field(payload, PayloadHeader::REMAINING_SUPPLY.start, PayloadHeader::REMAINING_SUPPLY.end, "remaining_supply")?;
        let op_byte = payload[PayloadHeader::OP.start];
        let op = NativeAssetOp::from_byte(op_byte).ok_or(CovenantError::InvalidPayloadOp { value: op_byte })?;
        let total_amount =
            decode_u64_field(payload, PayloadHeader::TOTAL_AMOUNT.start, PayloadHeader::TOTAL_AMOUNT.end, "total_amount")?;
        if is_mint_payload {
            if op != NativeAssetOp::Mint {
                return Err(CovenantError::InvalidPayloadOp { value: op_byte });
            }

            let output_amount = decode_u64_field(
                payload,
                MintPayloadLayout::OUTPUT0_AMOUNT.start,
                MintPayloadLayout::OUTPUT0_AMOUNT.end,
                "output_amount",
            )?;
            if output_amount == 0 {
                return Err(CovenantError::InvalidField("output_amount"));
            }

            let recipient_spk_bytes = decode_spk_bytes(
                payload,
                MintPayloadLayout::OUTPUT0_RECIPIENT.len.start,
                MintPayloadLayout::OUTPUT0_RECIPIENT.bytes.start,
                MintPayloadLayout::OUTPUT0_RECIPIENT.bytes.end,
                "recipient_spk_bytes",
            )?;

            if output_amount != total_amount {
                return Err(CovenantError::InvalidField("total_amount"));
            }

            let outputs = vec![NativeAssetOutput { amount: output_amount, recipient_spk_bytes }];
            return Ok(Self {
                asset_id,
                authority_spk_bytes,
                token_spk_bytes,
                remaining_supply,
                op,
                total_amount,
                input_amounts: Vec::new(),
                outputs,
            });
        }

        if op != NativeAssetOp::SplitMerge {
            return Err(CovenantError::InvalidPayloadOp { value: op_byte });
        }

        let mut input_amounts = Vec::with_capacity(MAX_INPUTS_COUNT);
        let mut seen_zero = false;
        for index in 0..MAX_INPUTS_COUNT {
            let (bytes_start, bytes_end) = input_amount_offsets(index);
            let amount = decode_u64_field(payload, bytes_start, bytes_end, "input_amount")?;
            if amount == 0 {
                seen_zero = true;
                continue;
            }
            if seen_zero {
                return Err(CovenantError::InvalidField("input_amount"));
            }
            input_amounts.push(amount);
        }
        if input_amounts.is_empty() {
            return Err(CovenantError::InvalidField("input_amount"));
        }

        let mut outputs = Vec::with_capacity(MAX_OUTPUTS_COUNT);
        let mut seen_zero_output = false;
        for index in 0..MAX_OUTPUTS_COUNT {
            let (bytes_start, bytes_end) = output_amount_offsets(index);
            let amount = decode_u64_field(payload, bytes_start, bytes_end, "output_amount")?;
            let (rec_len_start, rec_bytes_start, rec_bytes_end) = output_recipient_offsets(index);
            let rec_len = payload[rec_len_start] as usize;
            if amount == 0 {
                if rec_len != 0 {
                    return Err(CovenantError::InvalidField("recipient_spk_bytes"));
                }
                seen_zero_output = true;
                continue;
            }
            if seen_zero_output {
                return Err(CovenantError::InvalidField("output_amount"));
            }
            if rec_len == 0 {
                return Err(CovenantError::InvalidField("recipient_spk_bytes"));
            }
            let recipient_spk_bytes = decode_spk_bytes(payload, rec_len_start, rec_bytes_start, rec_bytes_end, "recipient_spk_bytes")?;
            outputs.push(NativeAssetOutput { amount, recipient_spk_bytes });
        }
        if outputs.is_empty() {
            return Err(CovenantError::InvalidField("output_amount"));
        }

        let total_inputs: u64 = input_amounts.iter().sum();
        let total_outputs: u64 = outputs.iter().map(|output| output.amount).sum();
        if total_inputs != total_amount || total_outputs != total_amount {
            return Err(CovenantError::InvalidField("total_amount"));
        }

        Ok(Self { asset_id, authority_spk_bytes, token_spk_bytes, remaining_supply, op, total_amount, input_amounts, outputs })
    }
}

/// spk bytes length is 36 or 37
fn validate_spk_len(len: usize, field: &'static str) -> Result<(), CovenantError> {
    if !(SPK_BYTES_MIN..=SPK_BYTES_MAX).contains(&len) {
        return Err(CovenantError::SpkBytesLengthOutOfRange { field, min: SPK_BYTES_MIN, max: SPK_BYTES_MAX, actual: len });
    }
    Ok(())
}

fn validate_spk_bytes(spk_bytes: &[u8], field: &'static str) -> Result<(), CovenantError> {
    validate_spk_len(spk_bytes.len(), field)
}

fn append_spk_bytes(payload: &mut Vec<u8>, spk_bytes: &[u8], field: &'static str) -> Result<(), CovenantError> {
    validate_spk_len(spk_bytes.len(), field)?;
    payload.push(spk_bytes.len() as u8);
    payload.extend_from_slice(spk_bytes);
    // fill with 0 if not = SPK_BYTES_MAX (static layout)
    payload.resize(payload.len() + (SPK_BYTES_MAX - spk_bytes.len()), 0);
    Ok(())
}

fn append_spk_bytes_padding(payload: &mut Vec<u8>) {
    payload.push(0);
    payload.resize(payload.len() + SPK_BYTES_MAX, 0);
}

fn decode_spk_bytes(
    payload: &[u8],
    len_start: usize,
    bytes_start: usize,
    bytes_end: usize,
    field: &'static str,
) -> Result<Vec<u8>, CovenantError> {
    let len = *payload.get(len_start).ok_or(CovenantError::InvalidField(field))? as usize;
    validate_spk_len(len, field)?;
    let bytes = payload.get(bytes_start..bytes_end).ok_or(CovenantError::InvalidField(field))?;

    // non 0 detected after supposedly finished spk bytes
    if bytes[len..].iter().any(|&b| b != 0) {
        return Err(CovenantError::InvalidField(field));
    }
    Ok(bytes[..len].to_vec())
}

fn decode_u64_field(payload: &[u8], start: usize, end: usize, field: &'static str) -> Result<u64, CovenantError> {
    let bytes = payload.get(start..end).ok_or(CovenantError::InvalidField(field))?;
    decode_u64_le(bytes, field)
}

fn output_recipient_offsets(index: usize) -> (usize, usize, usize) {
    let ranges = TransferPayloadLayout::output_recipient(index);
    (ranges.len.start, ranges.bytes.start, ranges.bytes.end)
}

fn input_amount_offsets(index: usize) -> (usize, usize) {
    let range = TransferPayloadLayout::input_amount(index);
    (range.start, range.end)
}

fn output_amount_offsets(index: usize) -> (usize, usize) {
    let range = TransferPayloadLayout::output_amount(index);
    (range.start, range.end)
}

/// Holds the current covenant UTXO state.
#[derive(Clone)]
pub struct NativeAssetState {
    knat_backtrace: KnatBacktrace,
    utxo_outpoint: TransactionOutpoint,
    utxo_entry: UtxoEntry,
    pub payload: NativeAssetPayload,
}

impl NativeAssetState {
    pub fn from_tx_with_entry_at_index(
        tx: Transaction,
        utxo_entry: UtxoEntry,
        knat_backtrace: KnatBacktrace,
        output_index: u32,
    ) -> CovenantResult<Self> {
        let payload = NativeAssetPayload::decode(&tx.payload)?;
        let outpoint = TransactionOutpoint::new(tx.id(), output_index);
        Ok(Self { knat_backtrace, utxo_outpoint: outpoint, utxo_entry, payload })
    }

    pub fn from_tx_with_entry_and_grandparent(
        tx: Transaction,
        utxo_entry: UtxoEntry,
        grandparent_tx: &Transaction,
    ) -> CovenantResult<Self> {
        Self::from_tx_with_entry_and_grandparent_at_index(tx, utxo_entry, grandparent_tx, 0)
    }

    pub fn from_tx_with_entry_and_grandparent_at_index(
        tx: Transaction,
        utxo_entry: UtxoEntry,
        grandparent_tx: &Transaction,
        output_index: u32,
    ) -> CovenantResult<Self> {
        let knat_backtrace = KnatBacktrace::from_parent_and_grandparent(&tx, grandparent_tx)?;
        Self::from_tx_with_entry_at_index(tx, utxo_entry, knat_backtrace, output_index)
    }

    pub fn utxo_entry(&self) -> &UtxoEntry {
        &self.utxo_entry
    }

    pub fn utxo_outpoint(&self) -> &TransactionOutpoint {
        &self.utxo_outpoint
    }

    fn build_sig_script(&self, covenant_script: &[u8]) -> CovenantResult<Vec<u8>> {
        build_sig_script_from_backtrace(&self.knat_backtrace, covenant_script)
    }
}

/// Holds witness fragments for KNAT parent/grandparent verification.
#[derive(Clone)]
pub struct KnatBacktrace {
    /// Grandparent preimage without payload.
    pub gp_preimage: Vec<u8>,
    /// Raw spk bytes (version + script) of grandparent output0 (kept separate for KNAT gating).
    pub gp_output0_script: Vec<u8>,
    /// Grandparent payload bytes.
    pub gp_payload: Vec<u8>,
    /// Parent preimage without payload (header + inputs + outputs).
    pub parent_preimage: Vec<u8>,
    /// Parent payload bytes.
    pub parent_payload: Vec<u8>,
}

impl KnatBacktrace {
    pub fn from_parent_and_grandparent(parent: &Transaction, grandparent: &Transaction) -> CovenantResult<Self> {
        let (parent_preimage, parent_payload) = split_preimage_payload(parent)?;
        let gp_parts = split_grandparent_preimage_for_knat(grandparent)?;
        Ok(Self {
            gp_preimage: gp_parts.preimage,
            gp_output0_script: gp_parts.output0_script,
            gp_payload: gp_parts.payload,
            parent_preimage,
            parent_payload,
        })
    }
}

fn split_preimage_payload(tx: &Transaction) -> CovenantResult<(Vec<u8>, Vec<u8>)> {
    // transaction_id_preimage returns payload at the end; split to reuse the prefix/payload separately.
    let preimage = transaction_id_preimage(tx);
    let payload_len = tx.payload.len();
    let split_at = preimage
        .len()
        .checked_sub(payload_len)
        // shouldn't happen
        .ok_or(CovenantError::PayloadLargerThanPreimage { payload_len, preimage_len: preimage.len() })?;
    let (preimage_prefix, payload_bytes) = preimage.split_at(split_at);
    Ok((preimage_prefix.to_vec(), payload_bytes.to_vec()))
}

struct GrandparentPreimageParts {
    preimage: Vec<u8>,
    output0_script: Vec<u8>,
    payload: Vec<u8>,
}

fn split_grandparent_preimage_for_knat(tx: &Transaction) -> CovenantResult<GrandparentPreimageParts> {
    let (preimage_without_payload, payload_bytes) = split_preimage_payload(tx)?;
    let output0 = tx.outputs.get(0).ok_or(CovenantError::MissingGrandparentOutput0)?;
    let out0_spk = output0.script_public_key.to_bytes();
    let out0_script = output0.script_public_key.script();

    // Output0 script is embedded in the preimage; validate its length.
    let prefix_len = TX_VERSION_SIZE
        + U64_SIZE
        + tx.inputs.len() * INPUT_NO_SIG_SCRIPT_SIZE
        + U64_SIZE
        + OUTPUT_VALUE_SIZE
        + SPK_VERSION_SIZE
        + SPK_SCRIPT_LEN_SIZE;
    let script_start = prefix_len;
    let script_end = script_start + out0_script.len();
    if preimage_without_payload.len() < script_end {
        return Err(CovenantError::GrandparentPreimageLengthMismatch {
            expected_len: script_end,
            actual_len: preimage_without_payload.len(),
        });
    }

    let script_len_slice = preimage_without_payload.get(prefix_len - SPK_SCRIPT_LEN_SIZE..prefix_len).ok_or(
        CovenantError::GrandparentPreimageLengthMismatch { expected_len: prefix_len, actual_len: preimage_without_payload.len() },
    )?;
    let script_len =
        u64::from_le_bytes(script_len_slice.try_into().map_err(|_| CovenantError::InvalidField("gp_output0_script_len"))?);
    let expected_len = out0_script.len() as u64;
    if script_len != expected_len {
        return Err(CovenantError::GrandparentOutputScriptLenMismatch { expected_len, actual_len: script_len });
    }

    Ok(GrandparentPreimageParts { preimage: preimage_without_payload, output0_script: out0_spk, payload: payload_bytes })
}

/// Build a signature script from KNAT backtrace fragments.
pub fn build_sig_script_from_backtrace(backtrace: &KnatBacktrace, covenant_script: &[u8]) -> CovenantResult<Vec<u8>> {
    // Stack order (top -> bottom) inside the covenant script after redeem: parent_payload, parent_preimage, gp_payload,
    // gp_output0_script, gp_preimage.

    // let chunks = covenant_script.split_at(250);

    let mut sb = ScriptBuilder::new();
    sb.add_data(&backtrace.gp_preimage)?
        .add_data(&backtrace.gp_output0_script)?
        .add_data(&backtrace.gp_payload)?
        .add_data(&backtrace.parent_preimage)?
        .add_data(&backtrace.parent_payload)?
        .add_data(covenant_script)?;

    // sb.add_data(chunks.0)?;
    // sb.add_data(chunks.1)?;

    Ok(sb.drain())
}

/// Verify parent/grandparent binding and leave a boolean for "continuation vs genesis".
pub fn knat_verify_parent_and_grandparent(sb: &mut ScriptBuilder) -> Result<(), ScriptBuilderError> {
    // Entry stack (top -> bottom), produced by build_sig_script_from_backtrace:
    // parent_payload: raw payload bytes
    // parent_preimage: transaction_id_preimage(parent) without payload
    // gp_payload: raw grandparent payload bytes
    // gp_output0_script: raw spk bytes (version + script) of grandparent output0
    // gp_preimage: grandparent preimage without payload
    //
    // Stack depths below are tied to this shape; 0 = top.
    const DEPTH_PARENT_PREIMAGE: i64 = 1;
    const DEPTH_PARENT_PREIMAGE_WITH_PREVOUT: i64 = 3;
    const DEPTH_GP_PREIMAGE_WITH_PARENT_PREVOUT: i64 = 7;
    const DEPTH_GP_PAYLOAD_WITH_PARENT_PREVOUT: i64 = 5;

    const DEPTH_GP_OUT0_SCRIPT_AFTER_KNAT_CHECK: i64 = 5;

    // --- 1) Bind the parent txid to the spending outpoint ---
    // Compute parent_txid = blake2b("TransactionID", parent_preimage || parent_payload),
    // then compare it to the txid referenced by the current input's outpoint.
    sb.add_op(Op2Dup)?
        .add_op(OpCat)?
        .add_data(b"TransactionID")?
        .add_op(OpBlake2bWithKey)?
        .add_op(OpTxInputIndex)?
        .add_op(OpOutpointTxId)?
        .add_op(OpEqualVerify)?;

    // --- 2) Extract parent input0 prevout (txid + index) from parent_preimage ---
    // Leaves prevout_txid and prevout_index on stack for:
    // - binding grandparent txid, and
    // - genesis asset_id derivation (prevout_txid || prevout_index).
    sb.add_i64(DEPTH_PARENT_PREIMAGE)?
        .add_op(OpPick)?
        .add_i64(PARENT_INPUT0_PREVOUT_TXID_START as i64)?
        .add_i64(PARENT_INPUT0_PREVOUT_TXID_END as i64)?
        .add_op(OpSubStr)?;
    sb.add_op(OpDup)?;

    sb.add_i64(DEPTH_PARENT_PREIMAGE_WITH_PREVOUT)?
        .add_op(OpPick)?
        .add_i64(PARENT_INPUT0_PREVOUT_INDEX_START as i64)?
        .add_i64(PARENT_INPUT0_PREVOUT_INDEX_END as i64)?
        .add_op(OpSubStr)?;

    // Arrange stack so prevout_txid sits above prevout_index for the gp binding below.
    sb.add_op(OpSwap)?;

    // --- 3) Bind grandparent txid to parent_prev_txid ---
    // Compute gp_txid = blake2b("TransactionID", gp_preimage || gp_payload)
    // and compare it to parent_prev_txid left on the stack.
    sb.add_i64(DEPTH_GP_PREIMAGE_WITH_PARENT_PREVOUT)?.add_op(OpPick)?;
    // After the previous pick, gp_payload is one item deeper.
    sb.add_i64(DEPTH_GP_PAYLOAD_WITH_PARENT_PREVOUT + 1)?.add_op(OpPick)?;
    sb.add_op(OpCat)?.add_data(b"TransactionID")?.add_op(OpBlake2bWithKey)?.add_op(OpEqualVerify)?;

    // --- 4) gate: compare gp_output0_script with current covenant spk bytes ---
    // Leaves a boolean on the stack for callers to branch on:
    // true = continuation (gp_output0_script matches this covenant), false = genesis.
    sb.add_i64(DEPTH_GP_OUT0_SCRIPT_AFTER_KNAT_CHECK)?.add_op(OpPick)?;
    sb.add_op(OpTxInputIndex)?.add_op(OpTxInputSpk)?.add_op(OpEqual)?;

    // Enforce input0 index == 0 only for genesis to keep asset_id derivation deterministic.
    sb.add_op(OpDup)?;
    sb.add_op(OpIf)?;
    sb.add_op(OpElse)?;
    sb.add_i64(1)?;
    sb.add_op(OpPick)?;
    sb.add_data(&[0u8, 0u8, 0u8, 0u8])?;
    sb.add_op(OpEqualVerify)?;
    sb.add_op(OpEndIf)?;

    // Stack after this function (top -> bottom):
    // is_continuation, prevout_index, prevout_txid, parent_payload, parent_preimage,
    // gp_payload, gp_output0_script, gp_preimage.
    Ok(())
}

// Verifies that the provided spk bytes match the current payload field (len byte + bytes prefix).
// Stack in: spk_bytes, ...
// Stack out: ... (consumes spk_bytes)
fn verify_spk_matches_current_payload(
    sb: &mut ScriptBuilder,
    len_start: usize,
    len_end: usize,
    bytes_start: usize,
    bytes_end: usize,
) -> Result<(), ScriptBuilderError> {
    sb.add_op(OpDup)?
        .add_op(OpSize)?
        .add_i64(len_start as i64)?
        .add_i64(len_end as i64)?
        .add_op(OpTxPayloadSubstr)?
        .add_op(OpSwap)?
        .add_op(OpEqualVerify)?;
    sb.add_op(OpDrop)?;
    sb.add_i64(len_start as i64)?.add_i64(len_end as i64)?.add_op(OpTxPayloadSubstr)?;
    sb.add_i64(bytes_start as i64)?.add_i64(bytes_end as i64)?.add_op(OpTxPayloadSubstr)?;
    sb.add_op(OpSwap)?;
    sb.add_i64(0)?;
    sb.add_op(OpSwap)?;
    sb.add_op(OpSubStr)?;
    sb.add_op(OpEqualVerify)?;
    Ok(())
}

// Verifies that the provided spk bytes match the parent payload field (len byte + bytes prefix).
// Stack in: spk_bytes, parent_payload, ...
// Stack out: parent_payload, ... (consumes spk_bytes)
fn verify_spk_matches_parent_payload(
    sb: &mut ScriptBuilder,
    len_start: usize,
    len_end: usize,
    bytes_start: usize,
    bytes_end: usize,
) -> Result<(), ScriptBuilderError> {
    sb.add_op(OpDup)?
        .add_op(OpSize)?
        .add_i64(3)?
        .add_op(OpPick)?
        .add_i64(len_start as i64)?
        .add_i64(len_end as i64)?
        .add_op(OpSubStr)?
        .add_op(OpSwap)?
        .add_op(OpEqualVerify)?;
    sb.add_op(OpDrop)?;
    sb.add_i64(1)?.add_op(OpPick)?.add_i64(bytes_start as i64)?.add_i64(bytes_end as i64)?.add_op(OpSubStr)?;
    sb.add_i64(2)?.add_op(OpPick)?.add_i64(len_start as i64)?.add_i64(len_end as i64)?.add_op(OpSubStr)?;
    sb.add_i64(0)?;
    sb.add_op(OpSwap)?;
    sb.add_op(OpSubStr)?;
    sb.add_op(OpEqualVerify)?;
    Ok(())
}

// Stack in: parent_payload, ...
// Stack out: ... (consumes parent_payload)
fn verify_parent_payload_magic_len(sb: &mut ScriptBuilder, expected_len: usize) -> Result<(), ScriptBuilderError> {
    sb.add_op(OpSize)?;
    sb.add_i64(expected_len as i64)?;
    sb.add_op(OpEqualVerify)?;
    sb.add_i64(PayloadHeader::MAGIC.start as i64)?;
    sb.add_i64(PayloadHeader::MAGIC.end as i64)?;
    sb.add_op(OpSubStr)?;
    sb.add_data(PAYLOAD_MAGIC)?;
    sb.add_op(OpEqualVerify)?;
    Ok(())
}

// No stack assumptions
// Stack out: [num, ...]
fn push_number_from_current_payload(
    sb: &mut ScriptBuilder,
    bytes_start: usize,
    bytes_end: usize,
) -> Result<(), ScriptBuilderError> {
    sb.add_i64(bytes_start as i64)?.add_i64(bytes_end as i64)?.add_op(OpTxPayloadSubstr)?;
    sb.add_op(OpBin2Num)?;
    Ok(())
}

// No stack assumptions
// Stack out: [num, ...]
fn push_input_amount_from_current_payload(sb: &mut ScriptBuilder, index: usize) -> Result<(), ScriptBuilderError> {
    let (bytes_start, bytes_end) = input_amount_offsets(index);
    push_number_from_current_payload(sb, bytes_start, bytes_end)
}

// No stack assumptions
// Stack out: [num, ...]
fn push_output_amount_from_current_payload(sb: &mut ScriptBuilder, index: usize) -> Result<(), ScriptBuilderError> {
    let (bytes_start, bytes_end) = output_amount_offsets(index);
    push_number_from_current_payload(sb, bytes_start, bytes_end)
}

// Stack in: parent_payload, ...
// Stack out: num, parent_payload, ...
fn push_number_from_parent_payload_on_top(
    sb: &mut ScriptBuilder,
    bytes_start: usize,
    bytes_end: usize,
) -> Result<(), ScriptBuilderError> {
    sb.add_op(OpDup)?.add_i64(bytes_start as i64)?.add_i64(bytes_end as i64)?.add_op(OpSubStr)?;
    sb.add_op(OpBin2Num)?;
    Ok(())
}

// no stack assumptions
// Stack out: [num, ...]
/// Pushes the current input's token amount by selecting the matching
/// indexed payload field for the active input, with a hard fail if the index is
/// out of range.
fn push_current_input_amount_by_index(sb: &mut ScriptBuilder) -> Result<(), ScriptBuilderError> {
    sb.add_op(OpTxInputIndex)?;
    sb.add_op(OpDup)?;
    sb.add_i64(0)?;
    sb.add_op(OpEqual)?;
    sb.add_op(OpIf)?;
    sb.add_op(OpDrop)?;
    let (bytes_start, bytes_end) = input_amount_offsets(0);
    push_number_from_current_payload(sb, bytes_start, bytes_end)?;
    sb.add_op(OpElse)?;
    sb.add_i64(1)?;
    sb.add_op(OpEqualVerify)?;
    let (bytes_start, bytes_end) = input_amount_offsets(1);
    push_number_from_current_payload(sb, bytes_start, bytes_end)?;
    sb.add_op(OpEndIf)?;
    Ok(())
}

// Stack in: [parent_output_index, parent_payload]
// Stack out: [num, ...]
/// Selects the matching output index and pushes its amount from
/// the parent payload with a hard fail if the index is out of range.
fn push_parent_output_amount_by_index(sb: &mut ScriptBuilder) -> Result<(), ScriptBuilderError> {
    sb.add_i64(1)?
        .add_op(OpPick)?
        .add_i64(PayloadHeader::OP.start as i64)?
        .add_i64(PayloadHeader::OP.end as i64)?
        .add_op(OpSubStr)?
        .add_ops(&[OpData1, TOKEN_OP_MINT])?
        .add_op(OpEqual)?;
    sb.add_op(OpIf)?;
    push_parent_output_amount_by_index_mint(sb)?;
    sb.add_op(OpElse)?;
    push_parent_output_amount_by_index_transfer(sb)?;
    sb.add_op(OpEndIf)?;
    Ok(())
}

fn push_parent_output_amount_by_index_mint(sb: &mut ScriptBuilder) -> Result<(), ScriptBuilderError> {
    sb.add_i64(0)?;
    sb.add_op(OpEqualVerify)?;
    push_number_from_parent_payload_on_top(sb, MintPayloadLayout::OUTPUT0_AMOUNT.start, MintPayloadLayout::OUTPUT0_AMOUNT.end)?;
    Ok(())
}

fn push_parent_output_amount_by_index_transfer(sb: &mut ScriptBuilder) -> Result<(), ScriptBuilderError> {
    sb.add_op(OpDup)?;
    sb.add_i64(0)?;
    sb.add_op(OpEqual)?;
    sb.add_op(OpIf)?;
    sb.add_op(OpDrop)?;
    let (bytes_start, bytes_end) = output_amount_offsets(0);
    push_number_from_parent_payload_on_top(sb, bytes_start, bytes_end)?;
    sb.add_op(OpElse)?;
    sb.add_i64(1)?;
    sb.add_op(OpEqualVerify)?;
    let (bytes_start, bytes_end) = output_amount_offsets(1);
    push_number_from_parent_payload_on_top(sb, bytes_start, bytes_end)?;
    sb.add_op(OpEndIf)?;
    Ok(())
}

// Stack in: [auth_output_index, parent_payload, ...]
// Stack out: [parent_payload, ...]
// Verifies that the parent payload recipient at auth_output_index matches the
// current auth input's script pubkey; fails if the index is out of range.
fn verify_parent_output_recipient_matches_auth_input(sb: &mut ScriptBuilder) -> Result<(), ScriptBuilderError> {
    sb.add_i64(1)?
        .add_op(OpPick)?
        .add_i64(PayloadHeader::OP.start as i64)?
        .add_i64(PayloadHeader::OP.end as i64)?
        .add_op(OpSubStr)?
        .add_ops(&[OpData1, TOKEN_OP_MINT])?
        .add_op(OpEqual)?;
    sb.add_op(OpIf)?;
    verify_parent_output_recipient_matches_auth_input_mint(sb)?;
    sb.add_op(OpElse)?;
    verify_parent_output_recipient_matches_auth_input_transfer(sb)?;
    sb.add_op(OpEndIf)?;
    Ok(())
}

fn verify_parent_output_recipient_matches_auth_input_mint(sb: &mut ScriptBuilder) -> Result<(), ScriptBuilderError> {
    sb.add_i64(0)?;
    sb.add_op(OpEqualVerify)?;
    sb.add_op(OpTxInputCount)?;
    sb.add_op(Op1Sub)?;
    sb.add_op(OpTxInputSpk)?;
    verify_spk_matches_parent_payload(
        sb,
        MintPayloadLayout::OUTPUT0_RECIPIENT.len.start,
        MintPayloadLayout::OUTPUT0_RECIPIENT.len.end,
        MintPayloadLayout::OUTPUT0_RECIPIENT.bytes.start,
        MintPayloadLayout::OUTPUT0_RECIPIENT.bytes.end,
    )?;
    Ok(())
}

fn verify_parent_output_recipient_matches_auth_input_transfer(sb: &mut ScriptBuilder) -> Result<(), ScriptBuilderError> {
    sb.add_op(OpDup)?;
    sb.add_i64(0)?;
    sb.add_op(OpEqual)?;
    sb.add_op(OpIf)?;
    sb.add_op(OpDrop)?;
    sb.add_op(OpTxInputCount)?;
    sb.add_op(Op1Sub)?;
    sb.add_op(OpTxInputSpk)?;
    let (len_start, bytes_start, bytes_end) = output_recipient_offsets(0);
    verify_spk_matches_parent_payload(sb, len_start, len_start + 1, bytes_start, bytes_end)?;
    sb.add_op(OpElse)?;
    sb.add_i64(1)?;
    sb.add_op(OpEqualVerify)?;
    sb.add_op(OpTxInputCount)?;
    sb.add_op(Op1Sub)?;
    sb.add_op(OpTxInputSpk)?;
    let (len_start, bytes_start, bytes_end) = output_recipient_offsets(1);
    verify_spk_matches_parent_payload(sb, len_start, len_start + 1, bytes_start, bytes_end)?;
    sb.add_op(OpEndIf)?;
    Ok(())
}

/// minter covenant enforcing mint payloads and covenant output checks.
/// Output1 is bound to the token covenant spk bytes carried in the payload (in the payload: to avoid circular deps minter<-->token covenants).
pub fn build_minter_covenant_script_knat20(authority_spk: &[u8]) -> Result<Vec<u8>, CovenantError> {
    let mut sb = ScriptBuilder::new();
    validate_spk_bytes(authority_spk, "authority_spk_bytes")?;

    knat_verify_parent_and_grandparent(&mut sb)?;

    // Stack after knat_verify (top -> bottom):
    // is_continuation, prevout_index, prevout_txid, parent_payload, parent_preimage, gp_payload, gp_output0_script, gp_preimage.
    //
    // KNAT gate result drives asset_id logic:
    // - continuation: drop prevout data and keep the asset_id from payload,
    // - genesis: recompute asset_id = prevout_txid || prevout_index and compare to payload.
    sb.add_op(OpIf)?;
    sb.add_op(OpDrop)?;
    sb.add_op(OpDrop)?;
    sb.add_op(OpElse)?;

    sb.add_op(OpCat)?;
    sb.add_i64(1)?
        .add_op(OpPick)?
        .add_i64(PayloadHeader::ASSET_ID.start as i64)?
        .add_i64(PayloadHeader::ASSET_ID.end as i64)?
        .add_op(OpSubStr)?
        .add_op(OpEqualVerify)?;
    sb.add_op(OpEndIf)?;

    // --- Parent payload header: validate length, magic ---
    sb.add_op(OpDup)?;
    verify_parent_payload_magic_len(&mut sb, MINT_PAYLOAD_LEN)?;

    // --- Payload header: validate length, magic ---
    sb.add_op(OpTxPayloadLen)?;
    sb.add_i64(MINT_PAYLOAD_LEN as i64)?;
    sb.add_op(OpEqualVerify)?;

    sb.add_i64(PayloadHeader::MAGIC.start as i64)?.add_i64(PayloadHeader::MAGIC.end as i64)?.add_op(OpTxPayloadSubstr)?;
    sb.add_data(PAYLOAD_MAGIC)?;
    sb.add_op(OpEqualVerify)?;

    // --- Current payload op = mint ---
    sb.add_i64(PayloadHeader::OP.start as i64)?.add_i64(PayloadHeader::OP.end as i64)?.add_op(OpTxPayloadSubstr)?;
    sb.add_ops(&[OpData1, TOKEN_OP_MINT])?;
    sb.add_op(OpEqualVerify)?;

    // --- Parent payload op = mint (mint chain can only follow mint) ---
    // note: on the first mint, this works because we ensure the genesis tx contains OP_MINT
    // an alternative would be to make the following check conditional based on GATE path (genesis vs continuation)
    // for the sake of simplicity and readability, i suggest we keep it as is
    sb.add_op(OpDup)?;
    sb.add_i64(PayloadHeader::OP.start as i64)?.add_i64(PayloadHeader::OP.end as i64)?.add_op(OpSubStr)?;
    sb.add_ops(&[OpData1, TOKEN_OP_MINT])?;
    sb.add_op(OpEqualVerify)?;

    // --- Fields that must be inherited from parent payload ---
    sb.add_op(OpDup)?
        .add_i64(PayloadHeader::ASSET_ID.start as i64)?
        .add_i64(PayloadHeader::TOKEN_SPK.bytes.end as i64)?
        .add_op(OpSubStr)?
        .add_i64(PayloadHeader::ASSET_ID.start as i64)?
        .add_i64(PayloadHeader::TOKEN_SPK.bytes.end as i64)?
        .add_op(OpTxPayloadSubstr)?
        .add_op(OpEqualVerify)?;

    // --- State transition verification: remaining_supply = parent_remaining_supply - total_amount ---
    // Numeric fields are fixed 8-byte LE values and converted via OP_BIN2NUM.

    // parent remaining supply from parent payload.
    push_number_from_parent_payload_on_top(&mut sb, PayloadHeader::REMAINING_SUPPLY.start, PayloadHeader::REMAINING_SUPPLY.end)?;

    // current total amount from current payload.
    push_number_from_current_payload(&mut sb, PayloadHeader::TOTAL_AMOUNT.start, PayloadHeader::TOTAL_AMOUNT.end)?;

    // current remaining supply from current payload.
    push_number_from_current_payload(&mut sb, PayloadHeader::REMAINING_SUPPLY.start, PayloadHeader::REMAINING_SUPPLY.end)?;

    // Stack now has (top to bottom):
    // current_remaining_supply, current_total_amount, parent_remaining_supply, parent_payload

    // Ensure minted amount > 0.
    sb.add_i64(1)?.add_op(OpPick)?;
    sb.add_i64(1)?;
    sb.add_op(OpGreaterThanOrEqual)?;
    sb.add_op(OpVerify)?;

    // Verify that parent_remaining_supply >= current_amount (amount doesn't exceed supply).
    sb.add_i64(2)?.add_op(OpPick)?; // parent_remaining_supply
    sb.add_i64(2)?.add_op(OpPick)?; // current_total_amount
    sb.add_op(OpGreaterThanOrEqual)?;
    sb.add_op(OpVerify)?;

    // Verify that current_remaining_supply == parent_remaining_supply - current_amount.
    sb.add_i64(2)?.add_op(OpPick)?; // parent_remaining_supply
    sb.add_i64(2)?.add_op(OpPick)?; // current_total_amount
    sb.add_op(OpSub)?;
    sb.add_i64(1)?.add_op(OpPick)?; // current_remaining_supply
    sb.add_op(OpNumEqualVerify)?;

    // Clean up stack - drop the supply check verification values.
    sb.add_op(OpDrop)?;
    sb.add_op(OpDrop)?;
    sb.add_op(OpDrop)?;

    // payload.total_amount must equal payload.output_amounts[0].
    push_number_from_current_payload(&mut sb, PayloadHeader::TOTAL_AMOUNT.start, PayloadHeader::TOTAL_AMOUNT.end)?;
    push_number_from_current_payload(&mut sb, MintPayloadLayout::OUTPUT0_AMOUNT.start, MintPayloadLayout::OUTPUT0_AMOUNT.end)?;
    sb.add_op(OpNumEqualVerify)?;

    // Output recipient spk length must be within bounds.
    sb.add_i64(MintPayloadLayout::OUTPUT0_RECIPIENT.len.start as i64)?
        .add_i64(MintPayloadLayout::OUTPUT0_RECIPIENT.len.end as i64)?
        .add_op(OpTxPayloadSubstr)?;
    sb.add_i64(SPK_BYTES_MIN as i64)?;
    sb.add_i64((SPK_BYTES_MAX + 1) as i64)?;
    sb.add_op(OpWithin)?;
    sb.add_op(OpVerify)?;

    // Authority spk bytes must match provided authority spk.
    // note: authorithy isn't transferable as of now, it could be the role of a future covenant OP
    sb.add_data(authority_spk)?;
    verify_spk_matches_current_payload(
        &mut sb,
        PayloadHeader::AUTHORITY_SPK.len.start,
        PayloadHeader::AUTHORITY_SPK.len.end,
        PayloadHeader::AUTHORITY_SPK.bytes.start,
        PayloadHeader::AUTHORITY_SPK.bytes.end,
    )?;

    // Authorization input and covenant outputs:
    // - input[1] must spend the authority_spk,
    // - output[0] must loop back to this covenant,
    // - output[1] must be a token covenant whose script bytes match payload.token_spk_bytes.
    sb.add_op(OpTxInputCount)?;
    sb.add_i64(2)?;
    sb.add_op(OpGreaterThanOrEqual)?;
    sb.add_op(OpVerify)?;

    sb.add_i64(1)?;
    sb.add_op(OpTxInputSpk)?;
    sb.add_data(authority_spk)?;
    sb.add_op(OpEqualVerify)?;

    sb.add_op(OpTxInputIndex)?.add_op(OpTxInputSpk)?.add_i64(0)?.add_op(OpTxOutputSpk)?.add_op(OpEqualVerify)?;

    sb.add_i64(1)?.add_op(OpTxOutputSpk)?;
    verify_spk_matches_current_payload(
        &mut sb,
        PayloadHeader::TOKEN_SPK.len.start,
        PayloadHeader::TOKEN_SPK.len.end,
        PayloadHeader::TOKEN_SPK.bytes.start,
        PayloadHeader::TOKEN_SPK.bytes.end,
    )?;

    sb.add_op(OpTxOutputCount)?.add_i64(2)?.add_op(OpGreaterThanOrEqual)?;
    sb.add_op(OpVerify)?;

    // Drop KNAT backtrace items (parent_payload, parent_preimage, gp_payload, gp_output0_script, gp_preimage) and leave true.
    sb.add_op(Op2Drop)?;
    sb.add_op(Op2Drop)?;
    sb.add_op(OpDrop)?;
    sb.add_op(OpTrue)?;

    Ok(sb.drain())
}

/// Token covenant script. `minter_covenant_spk` binds mint-origin to the minter covenant.
pub fn build_token_covenant_script_knat20(minter_covenant_spk: &[u8]) -> Result<Vec<u8>, CovenantError> {
    let mut sb = ScriptBuilder::new();

    // build time validation check
    validate_spk_bytes(minter_covenant_spk, "minter_covenant_spk")?;
    let minter_covenant_hash = TransactionID::hash(minter_covenant_spk);
    let minter_covenant_hash_bytes = minter_covenant_hash.as_bytes();

    knat_verify_parent_and_grandparent(&mut sb)?;

    // --- Payload header: validate length, magic ---
    sb.add_op(OpTxPayloadLen)?;
    sb.add_i64(TRANSFER_PAYLOAD_LEN as i64)?;
    sb.add_op(OpEqualVerify)?;

    sb.add_i64(PayloadHeader::MAGIC.start as i64)?.add_i64(PayloadHeader::MAGIC.end as i64)?.add_op(OpTxPayloadSubstr)?;
    sb.add_data(PAYLOAD_MAGIC)?;
    sb.add_op(OpEqualVerify)?;

    // Stack after knat_verify (top -> bottom):
    // is_continuation, prevout_index, prevout_txid, parent_payload, parent_preimage, gp_payload, gp_output0_script, gp_preimage.
    //
    // Depths below assume this stack shape plus the duplicated continuation flag; 0 = top.
    const DEPTH_PARENT_PAYLOAD_WITH_PREVOUT: i64 = 3;
    const DEPTH_GP_OUT0_SCRIPT_WITH_PREVOUT: i64 = 6;

    // Duplicate continuation flag to reuse later for parent output index mapping.
    sb.add_op(OpDup)?;

    // Genesis vs continuation:
    // - continuation: parent op must be split/merge,
    // - genesis: parent op must be mint and gp_output0_script hash must match the minter covenant spk hash.
    sb.add_op(OpIf)?;

    sb.add_i64(DEPTH_PARENT_PAYLOAD_WITH_PREVOUT)?
        .add_op(OpPick)?
        .add_i64(PayloadHeader::OP.start as i64)?
        .add_i64(PayloadHeader::OP.end as i64)?
        .add_op(OpSubStr)?;
    sb.add_data(&[TOKEN_OP_SPLIT_MERGE])?;
    sb.add_op(OpEqualVerify)?;
    sb.add_i64(DEPTH_PARENT_PAYLOAD_WITH_PREVOUT)?.add_op(OpPick)?;
    verify_parent_payload_magic_len(&mut sb, TRANSFER_PAYLOAD_LEN)?;

    sb.add_op(OpElse)?;

    sb.add_i64(DEPTH_PARENT_PAYLOAD_WITH_PREVOUT)?
        .add_op(OpPick)?
        .add_i64(PayloadHeader::OP.start as i64)?
        .add_i64(PayloadHeader::OP.end as i64)?
        .add_op(OpSubStr)?;
    sb.add_ops(&[OpData1, TOKEN_OP_MINT])?;
    sb.add_op(OpEqualVerify)?;
    sb.add_i64(DEPTH_PARENT_PAYLOAD_WITH_PREVOUT)?.add_op(OpPick)?;
    verify_parent_payload_magic_len(&mut sb, MINT_PAYLOAD_LEN)?;

    sb.add_i64(DEPTH_GP_OUT0_SCRIPT_WITH_PREVOUT)?.add_op(OpPick)?;
    sb.add_data(b"TransactionID")?;
    sb.add_op(OpBlake2bWithKey)?;
    sb.add_data(&minter_covenant_hash_bytes)?;
    sb.add_op(OpEqualVerify)?;
    sb.add_op(OpEndIf)?;

    // Drop prevout_index and prevout_txid, keep continuation flag and parent payload.
    sb.add_op(OpSwap)?;
    sb.add_op(OpDrop)?;
    sb.add_op(OpSwap)?;
    sb.add_op(OpDrop)?;

    // --- Current payload op = split/merge ---
    sb.add_i64(PayloadHeader::OP.start as i64)?.add_i64(PayloadHeader::OP.end as i64)?.add_op(OpTxPayloadSubstr)?;
    sb.add_data(&[TOKEN_OP_SPLIT_MERGE])?;
    sb.add_op(OpEqualVerify)?;

    // --- Fields that must be inherited from parent payload ---
    sb.add_i64(1)?
        .add_op(OpPick)?
        .add_i64(PayloadHeader::ASSET_ID.start as i64)?
        .add_i64(PayloadHeader::REMAINING_SUPPLY.end as i64)?
        .add_op(OpSubStr)?
        .add_i64(PayloadHeader::ASSET_ID.start as i64)?
        .add_i64(PayloadHeader::REMAINING_SUPPLY.end as i64)?
        .add_op(OpTxPayloadSubstr)?
        .add_op(OpEqualVerify)?;

    // number of inputs must be within bounds (token inputs + auth input).
    sb.add_op(OpTxInputCount)?;
    sb.add_i64(2)?;
    sb.add_i64((MAX_INPUTS_COUNT + 2) as i64)?;
    sb.add_op(OpWithin)?;
    sb.add_op(OpVerify)?;

    // number of outputs must be within bounds.
    sb.add_op(OpTxOutputCount)?;
    sb.add_i64(1)?;
    sb.add_i64((MAX_OUTPUTS_COUNT + 1) as i64)?;
    sb.add_op(OpWithin)?;
    sb.add_op(OpVerify)?;

    // Optional second input amount must match the input count.
    sb.add_op(OpTxInputCount)?;
    sb.add_i64(2)?;
    sb.add_op(OpEqual)?;
    sb.add_op(OpIf)?;
    push_input_amount_from_current_payload(&mut sb, 1)?;
    sb.add_i64(0)?;
    sb.add_op(OpNumEqualVerify)?;
    sb.add_op(OpElse)?;
    push_input_amount_from_current_payload(&mut sb, 1)?;
    sb.add_i64(1)?;
    sb.add_op(OpGreaterThanOrEqual)?;
    sb.add_op(OpVerify)?;
    sb.add_op(OpEndIf)?;

    let (rec0_len_start, _, _) = output_recipient_offsets(0);
    let (rec1_len_start, _, _) = output_recipient_offsets(1);

    // Output0 amount must be > 0 and recipient length must be within bounds.
    push_output_amount_from_current_payload(&mut sb, 0)?;
    sb.add_i64(1)?;
    sb.add_op(OpGreaterThanOrEqual)?;
    sb.add_op(OpVerify)?;
    sb.add_i64(rec0_len_start as i64)?.add_i64((rec0_len_start + 1) as i64)?.add_op(OpTxPayloadSubstr)?;
    sb.add_i64(SPK_BYTES_MIN as i64)?;
    sb.add_i64((SPK_BYTES_MAX + 1) as i64)?;
    sb.add_op(OpWithin)?;
    sb.add_op(OpVerify)?;

    // Output1 amount/recipient must match the output count.
    sb.add_op(OpTxOutputCount)?;
    sb.add_i64(1)?;
    sb.add_op(OpEqual)?;
    sb.add_op(OpIf)?;
    push_output_amount_from_current_payload(&mut sb, 1)?;
    sb.add_i64(0)?;
    sb.add_op(OpNumEqualVerify)?;
    sb.add_i64(rec1_len_start as i64)?.add_i64((rec1_len_start + 1) as i64)?.add_op(OpTxPayloadSubstr)?;
    sb.add_data(&[0u8])?;
    sb.add_op(OpEqualVerify)?;
    sb.add_op(OpElse)?;
    push_output_amount_from_current_payload(&mut sb, 1)?;
    sb.add_i64(1)?;
    sb.add_op(OpGreaterThanOrEqual)?;
    sb.add_op(OpVerify)?;
    sb.add_i64(rec1_len_start as i64)?.add_i64((rec1_len_start + 1) as i64)?.add_op(OpTxPayloadSubstr)?;
    sb.add_i64(SPK_BYTES_MIN as i64)?;
    sb.add_i64((SPK_BYTES_MAX + 1) as i64)?;
    sb.add_op(OpWithin)?;
    sb.add_op(OpVerify)?;
    sb.add_op(OpEndIf)?;

    // Amount conservation: sum(inputs) == sum(outputs).
    push_input_amount_from_current_payload(&mut sb, 0)?;
    push_input_amount_from_current_payload(&mut sb, 1)?;
    sb.add_op(OpAdd)?;
    push_output_amount_from_current_payload(&mut sb, 0)?;
    push_output_amount_from_current_payload(&mut sb, 1)?;
    sb.add_op(OpAdd)?;
    sb.add_op(OpNumEqualVerify)?;

    // Use current input outpoint index as parent output index; genesis uses outpoint_index - 1.
    sb.add_op(OpTxInputIndex)?.add_op(OpOutpointIndex)?;
    sb.add_op(OpSwap)?;
    sb.add_op(OpIf)?;
    sb.add_op(OpElse)?;
    sb.add_op(Op1Sub)?;
    sb.add_op(OpEndIf)?;

    // Verify parent output amount matches the current input amount and amount > 0.
    sb.add_op(OpTuck)?; // keep parent_output_index for auth check
    push_parent_output_amount_by_index(&mut sb)?;
    push_current_input_amount_by_index(&mut sb)?;
    sb.add_op(OpDup)?;
    sb.add_i64(1)?;
    sb.add_op(OpGreaterThanOrEqual)?;
    sb.add_op(OpVerify)?;
    sb.add_op(OpNumEqualVerify)?;

    // Authorization input spk must match the parent recipient for this output index.
    sb.add_op(OpSwap)?;
    verify_parent_output_recipient_matches_auth_input(&mut sb)?;

    // Ensure outputs are token covenant outputs.
    sb.add_op(OpTxOutputCount)?;
    sb.add_i64(1)?;
    sb.add_op(OpEqual)?;
    sb.add_op(OpIf)?;
    sb.add_op(OpTxInputIndex)?.add_op(OpTxInputSpk)?;
    sb.add_i64(0)?.add_op(OpTxOutputSpk)?;
    sb.add_op(OpEqualVerify)?;
    sb.add_op(OpElse)?;
    sb.add_op(OpTxInputIndex)?.add_op(OpTxInputSpk)?;
    sb.add_op(OpDup)?;
    sb.add_i64(0)?.add_op(OpTxOutputSpk)?;
    sb.add_op(OpEqualVerify)?;
    sb.add_i64(1)?.add_op(OpTxOutputSpk)?;
    sb.add_op(OpEqualVerify)?;
    sb.add_op(OpEndIf)?;

    // Drop KNAT backtrace items (parent_payload, parent_preimage, gp_payload, gp_output0_script, gp_preimage) and leave true.
    sb.add_op(Op2Drop)?;
    sb.add_op(Op2Drop)?;
    sb.add_op(OpDrop)?;
    sb.add_op(OpTrue)?;

    Ok(sb.drain())
}

/// Build a mint transaction with an authorization input at index 1.
/// Uses the provided mass calculator to estimate fees.
pub fn build_mint_tx(
    state: &NativeAssetState,
    next_payload: &NativeAssetPayload,
    minter_spk: &ScriptPublicKey,
    token_spk: &ScriptPublicKey,
    token_value: u64,
    auth_input: TransactionInput,
    auth_entry: UtxoEntry,
    minter_covenant_script: &[u8],
    mass_calculator: &MassCalculator,
) -> CovenantResult<Transaction> {
    let payload = next_payload.encode()?;
    let minter_sig_script = state.build_sig_script(minter_covenant_script)?;

    let minter_input = TransactionInput::new(*state.utxo_outpoint(), minter_sig_script, 0, 0);

    let temp_minter_value = checked_sub_or_err(state.utxo_entry.amount, token_value)?;
    let temp_outputs =
        vec![TransactionOutput::new(temp_minter_value, minter_spk.clone()), TransactionOutput::new(token_value, token_spk.clone())];
    let temp_tx = Transaction::new(
        0,
        vec![minter_input.clone(), auth_input.clone()],
        temp_outputs,
        0,
        SubnetworkId::default(),
        0,
        payload.clone(),
    );
    let temp_tx = PopulatedTransaction::new(&temp_tx, vec![state.utxo_entry.clone(), auth_entry.clone()]);

    let mass = estimate_mass(mass_calculator, &temp_tx);

    let minter_value = checked_sub_or_err(temp_minter_value, mass)?;
    let outputs =
        vec![TransactionOutput::new(minter_value, minter_spk.clone()), TransactionOutput::new(token_value, token_spk.clone())];

    let mut tx = Transaction::new(0, vec![minter_input, auth_input], outputs, 0, SubnetworkId::default(), 0, payload);
    tx.finalize();
    Ok(tx)
}

/// Build a token transfer transaction with an authorization input at index 1.
/// Uses the provided mass calculator to estimate fees.
pub fn build_token_transfer_tx(
    state: &NativeAssetState,
    next_payload: &NativeAssetPayload,
    token_spk: &ScriptPublicKey,
    auth_input: TransactionInput,
    auth_entry: UtxoEntry,
    token_covenant_script: &[u8],
    mass_calculator: &MassCalculator,
) -> CovenantResult<Transaction> {
    let payload = next_payload.encode()?;
    let token_sig_script = state.build_sig_script(token_covenant_script)?;

    let token_input = TransactionInput::new(*state.utxo_outpoint(), token_sig_script, 0, 0);

    let temp_token_output = TransactionOutput::new(state.utxo_entry.amount, token_spk.clone());
    let temp_tx = Transaction::new(
        0,
        vec![token_input.clone(), auth_input.clone()],
        vec![temp_token_output],
        0,
        SubnetworkId::default(),
        0,
        payload.clone(),
    );
    let temp_tx = PopulatedTransaction::new(&temp_tx, vec![state.utxo_entry.clone(), auth_entry.clone()]);

    let mass = estimate_mass(mass_calculator, &temp_tx);

    let token_value = checked_sub_or_err(state.utxo_entry.amount, mass)?;
    let output = TransactionOutput::new(token_value, token_spk.clone());

    let mut tx = Transaction::new(0, vec![token_input, auth_input], vec![output], 0, SubnetworkId::default(), 0, payload);
    tx.finalize();
    Ok(tx)
}

fn estimate_mass(calculator: &MassCalculator, tx: &PopulatedTransaction<'_>) -> u64 {
    let storage_mass = calculator.calc_contextual_masses(tx).map(|mass| mass.storage_mass).unwrap_or_default();
    let NonContextualMasses { compute_mass, transient_mass } = calculator.calc_non_contextual_masses(tx.tx);
    storage_mass.max(compute_mass).max(transient_mass)
}

fn checked_sub_or_err(available: u64, required: u64) -> CovenantResult<u64> {
    available.checked_sub(required).ok_or(CovenantError::InsufficientFunds { available, required })
}

/// outpoint_txid || outpoint_index_le
/// returns the asset_id for the outpoint
pub fn asset_id_for_outpoint(outpoint: &TransactionOutpoint) -> [u8; ASSET_ID_SIZE] {
    let mut out = [0u8; ASSET_ID_SIZE];
    out[..OUTPOINT_TXID_SIZE].copy_from_slice(outpoint.transaction_id.as_ref());
    out[OUTPOINT_TXID_SIZE..].copy_from_slice(&outpoint.index.to_le_bytes());
    out
}

/// spk.to_bytes() (version + script)
pub fn try_spk_bytes(spk: &ScriptPublicKey) -> CovenantResult<Vec<u8>> {
    let spk_bytes = spk.to_bytes();
    validate_spk_bytes(&spk_bytes, "spk_bytes")?;
    Ok(spk_bytes)
}
