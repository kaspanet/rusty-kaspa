use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use workflow_serializer::prelude::*;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, Copy)]
#[borsh(use_discriminant = true)]
#[repr(i32)]
pub enum RpcDataVerbosityLevel {
    None = 0,
    Low = 1,
    High = 2,
    Full = 3,
}

impl Serializer for RpcDataVerbosityLevel {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;

        let val: i32 = *self as i32;
        writer.write_all(&val.to_le_bytes())?;

        Ok(())
    }
}

impl Deserializer for RpcDataVerbosityLevel {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;

        let mut buf = [0u8; 4];
        reader.read_exact(&mut buf)?;
        let val = i32::from_le_bytes(buf);
        RpcDataVerbosityLevel::try_from(val)
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid RpcDataVerbosityLevel"))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RpcHeaderVerbosity {
    /// Cached hash
    pub include_hash: Option<bool>,
    pub include_version: Option<bool>,
    pub include_parents_by_level: Option<bool>,
    pub include_hash_merkle_root: Option<bool>,
    pub include_accepted_id_merkle_root: Option<bool>,
    pub include_utxo_commitment: Option<bool>,
    /// Timestamp is in milliseconds
    pub include_timestamp: Option<bool>,
    pub include_bits: Option<bool>,
    pub include_nonce: Option<bool>,
    pub include_daa_score: Option<bool>,
    pub include_blue_work: Option<bool>,
    pub include_blue_score: Option<bool>,
    pub include_pruning_point: Option<bool>,
}

impl Serializer for RpcHeaderVerbosity {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;

        store!(Option<bool>, &self.include_hash, writer)?;
        store!(Option<bool>, &self.include_version, writer)?;
        store!(Option<bool>, &self.include_parents_by_level, writer)?;
        store!(Option<bool>, &self.include_hash_merkle_root, writer)?;
        store!(Option<bool>, &self.include_accepted_id_merkle_root, writer)?;
        store!(Option<bool>, &self.include_utxo_commitment, writer)?;
        store!(Option<bool>, &self.include_timestamp, writer)?;
        store!(Option<bool>, &self.include_bits, writer)?;
        store!(Option<bool>, &self.include_nonce, writer)?;
        store!(Option<bool>, &self.include_daa_score, writer)?;
        store!(Option<bool>, &self.include_blue_work, writer)?;
        store!(Option<bool>, &self.include_blue_score, writer)?;
        store!(Option<bool>, &self.include_pruning_point, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcHeaderVerbosity {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;

        let include_hash = load!(Option<bool>, reader)?;
        let include_version = load!(Option<bool>, reader)?;
        let include_parents_by_level = load!(Option<bool>, reader)?;
        let include_hash_merkle_root = load!(Option<bool>, reader)?;
        let include_accepted_id_merkle_root = load!(Option<bool>, reader)?;
        let include_utxo_commitment = load!(Option<bool>, reader)?;
        let include_timestamp = load!(Option<bool>, reader)?;
        let include_bits = load!(Option<bool>, reader)?;
        let include_nonce = load!(Option<bool>, reader)?;
        let include_daa_score = load!(Option<bool>, reader)?;
        let include_blue_work = load!(Option<bool>, reader)?;
        let include_blue_score = load!(Option<bool>, reader)?;
        let include_pruning_point = load!(Option<bool>, reader)?;

        Ok(Self {
            include_hash,
            include_version,
            include_parents_by_level,
            include_hash_merkle_root,
            include_accepted_id_merkle_root,
            include_utxo_commitment,
            include_timestamp,
            include_bits,
            include_nonce,
            include_daa_score,
            include_blue_work,
            include_blue_score,
            include_pruning_point,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RpcUtxoEntryVerboseDataVerbosity {
    pub include_script_public_key_type: Option<bool>,
    pub include_script_public_key_address: Option<bool>,
}

impl RpcUtxoEntryVerboseDataVerbosity {
    pub fn new(include_script_public_key_type: Option<bool>, include_script_public_key_address: Option<bool>) -> Self {
        Self { include_script_public_key_type, include_script_public_key_address }
    }
}

impl Serializer for RpcUtxoEntryVerboseDataVerbosity {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(Option<bool>, &self.include_script_public_key_type, writer)?;
        store!(Option<bool>, &self.include_script_public_key_address, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcUtxoEntryVerboseDataVerbosity {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let include_script_public_key_type = load!(Option<bool>, reader)?;
        let include_script_public_key_address = load!(Option<bool>, reader)?;

        Ok(Self { include_script_public_key_type, include_script_public_key_address })
    }
}

// RpcUtxoEntryVerbosity
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RpcUtxoEntryVerbosity {
    pub include_amount: Option<bool>,
    pub include_script_public_key: Option<bool>,
    pub include_block_daa_score: Option<bool>,
    pub include_is_coinbase: Option<bool>,
    pub verbose_data_verbosity: Option<RpcUtxoEntryVerboseDataVerbosity>,
}

impl RpcUtxoEntryVerbosity {
    pub fn new(
        include_amount: Option<bool>,
        include_script_public_key: Option<bool>,
        include_block_daa_score: Option<bool>,
        include_is_coinbase: Option<bool>,
        verbose_data_verbosity: Option<RpcUtxoEntryVerboseDataVerbosity>,
    ) -> Self {
        Self { include_amount, include_script_public_key, include_block_daa_score, include_is_coinbase, verbose_data_verbosity }
    }
}

impl Serializer for RpcUtxoEntryVerbosity {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(Option<bool>, &self.include_amount, writer)?;
        store!(Option<bool>, &self.include_script_public_key, writer)?;
        store!(Option<bool>, &self.include_block_daa_score, writer)?;
        store!(Option<bool>, &self.include_is_coinbase, writer)?;
        serialize!(Option<RpcUtxoEntryVerboseDataVerbosity>, &self.verbose_data_verbosity, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcUtxoEntryVerbosity {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;

        let include_amount = load!(Option<bool>, reader)?;
        let include_script_public_key = load!(Option<bool>, reader)?;
        let include_block_daa_score = load!(Option<bool>, reader)?;
        let include_is_coinbase = load!(Option<bool>, reader)?;
        let verbose_data_verbosity = deserialize!(Option<RpcUtxoEntryVerboseDataVerbosity>, reader)?;

        Ok(Self { include_amount, include_script_public_key, include_block_daa_score, include_is_coinbase, verbose_data_verbosity })
    }
}

// RpcTransactionInputVerbosity
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionInputVerbosity {
    pub include_previous_outpoint: Option<bool>,
    pub include_signature_script: Option<bool>,
    pub include_sequence: Option<bool>,
    pub include_sig_op_count: Option<bool>,
    pub verbose_data_verbosity: Option<RpcTransactionInputVerboseDataVerbosity>,
}

impl RpcTransactionInputVerbosity {
    pub fn new(
        include_previous_outpoint: Option<bool>,
        include_signature_script: Option<bool>,
        include_sequence: Option<bool>,
        include_sig_op_count: Option<bool>,
        verbose_data_verbosity: Option<RpcTransactionInputVerboseDataVerbosity>,
    ) -> Self {
        Self { include_previous_outpoint, include_signature_script, include_sequence, include_sig_op_count, verbose_data_verbosity }
    }
}

impl Serializer for RpcTransactionInputVerbosity {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(Option<bool>, &self.include_previous_outpoint, writer)?;
        store!(Option<bool>, &self.include_signature_script, writer)?;
        store!(Option<bool>, &self.include_sequence, writer)?;
        store!(Option<bool>, &self.include_sig_op_count, writer)?;
        serialize!(Option<RpcTransactionInputVerboseDataVerbosity>, &self.verbose_data_verbosity, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionInputVerbosity {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;

        let include_previous_outpoint = load!(Option<bool>, reader)?;
        let include_signature_script = load!(Option<bool>, reader)?;
        let include_sequence = load!(Option<bool>, reader)?;
        let include_sig_op_count = load!(Option<bool>, reader)?;
        let verbose_data_verbosity = deserialize!(Option<RpcTransactionInputVerboseDataVerbosity>, reader)?;

        Ok(Self {
            include_previous_outpoint,
            include_signature_script,
            include_sequence,
            include_sig_op_count,
            verbose_data_verbosity,
        })
    }
}

// RpcTransactionInputVerboseDataVerbosity
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionInputVerboseDataVerbosity {
    pub utxo_entry_verbosity: Option<RpcUtxoEntryVerbosity>,
}

impl RpcTransactionInputVerboseDataVerbosity {
    pub fn new(utxo_entry_verbosity: Option<RpcUtxoEntryVerbosity>) -> Self {
        Self { utxo_entry_verbosity }
    }
}

impl Serializer for RpcTransactionInputVerboseDataVerbosity {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        serialize!(Option<RpcUtxoEntryVerbosity>, &self.utxo_entry_verbosity, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionInputVerboseDataVerbosity {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let utxo_entry_verbosity = deserialize!(Option<RpcUtxoEntryVerbosity>, reader)?;

        Ok(Self { utxo_entry_verbosity })
    }
}

// RpcTransactionOutputVerbosity

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionOutputVerbosity {
    pub include_amount: Option<bool>,
    pub include_script_public_key: Option<bool>,
    pub verbose_data_verbosity: Option<RpcTransactionOutputVerboseDataVerbosity>,
}

impl RpcTransactionOutputVerbosity {
    pub fn new(
        include_amount: Option<bool>,
        include_script_public_key: Option<bool>,
        verbose_data_verbosity: Option<RpcTransactionOutputVerboseDataVerbosity>,
    ) -> Self {
        Self { include_amount, include_script_public_key, verbose_data_verbosity }
    }
}

impl Serializer for RpcTransactionOutputVerbosity {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(Option<bool>, &self.include_amount, writer)?;
        store!(Option<bool>, &self.include_script_public_key, writer)?;
        serialize!(Option<RpcTransactionOutputVerboseDataVerbosity>, &self.verbose_data_verbosity, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionOutputVerbosity {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;

        let include_amount = load!(Option<bool>, reader)?;
        let include_script_public_key = load!(Option<bool>, reader)?;
        let verbose_data_verbosity = deserialize!(Option<RpcTransactionOutputVerboseDataVerbosity>, reader)?;

        Ok(Self { include_amount, include_script_public_key, verbose_data_verbosity })
    }
}

// RpcTransactionOutputVerboseDataVerbosity
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionOutputVerboseDataVerbosity {
    pub include_script_public_key_type: Option<bool>,
    pub include_script_public_key_address: Option<bool>,
}

impl RpcTransactionOutputVerboseDataVerbosity {
    pub fn new(include_script_public_key_type: Option<bool>, include_script_public_key_address: Option<bool>) -> Self {
        Self { include_script_public_key_type, include_script_public_key_address }
    }
}

impl Serializer for RpcTransactionOutputVerboseDataVerbosity {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(Option<bool>, &self.include_script_public_key_type, writer)?;
        store!(Option<bool>, &self.include_script_public_key_address, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionOutputVerboseDataVerbosity {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;

        let include_script_public_key_type = load!(Option<bool>, reader)?;
        let include_script_public_key_address = load!(Option<bool>, reader)?;

        Ok(Self { include_script_public_key_type, include_script_public_key_address })
    }
}

// RpcTransactionVerbosity
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionVerbosity {
    pub include_version: Option<bool>,
    pub input_verbosity: Option<RpcTransactionInputVerbosity>,
    pub output_verbosity: Option<RpcTransactionOutputVerbosity>,
    pub include_lock_time: Option<bool>,
    pub include_subnetwork_id: Option<bool>,
    pub include_gas: Option<bool>,
    pub include_payload: Option<bool>,
    pub include_mass: Option<bool>,
    pub verbose_data_verbosity: Option<RpcTransactionVerboseDataVerbosity>,
}

impl RpcTransactionVerbosity {
    pub fn new(
        include_version: Option<bool>,
        input_verbosity: Option<RpcTransactionInputVerbosity>,
        output_verbosity: Option<RpcTransactionOutputVerbosity>,
        include_lock_time: Option<bool>,
        include_subnetwork_id: Option<bool>,
        include_gas: Option<bool>,
        include_payload: Option<bool>,
        include_mass: Option<bool>,
        verbose_data_verbosity: Option<RpcTransactionVerboseDataVerbosity>,
    ) -> Self {
        Self {
            include_version,
            input_verbosity,
            output_verbosity,
            include_lock_time,
            include_subnetwork_id,
            include_gas,
            include_payload,
            include_mass,
            verbose_data_verbosity,
        }
    }

    pub fn requires_populated_transaction(&self) -> bool {
        self.input_verbosity
            .as_ref()
            .is_some_and(|active| active.verbose_data_verbosity.as_ref().is_some_and(|active| active.utxo_entry_verbosity.is_some()))
    }

    pub fn requires_block_hash(&self) -> bool {
        self.verbose_data_verbosity.as_ref().is_some_and(|active| active.include_block_hash.is_some())
    }

    pub fn requires_block_time(&self) -> bool {
        self.verbose_data_verbosity.as_ref().is_some_and(|active| active.include_block_time.is_some())
    }
}

impl Serializer for RpcTransactionVerbosity {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(Option<bool>, &self.include_version, writer)?;
        serialize!(Option<RpcTransactionInputVerbosity>, &self.input_verbosity, writer)?;
        serialize!(Option<RpcTransactionOutputVerbosity>, &self.output_verbosity, writer)?;
        store!(Option<bool>, &self.include_lock_time, writer)?;
        store!(Option<bool>, &self.include_subnetwork_id, writer)?;
        store!(Option<bool>, &self.include_gas, writer)?;
        store!(Option<bool>, &self.include_payload, writer)?;
        store!(Option<bool>, &self.include_mass, writer)?;
        serialize!(Option<RpcTransactionVerboseDataVerbosity>, &self.verbose_data_verbosity, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionVerbosity {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;

        let include_version = load!(Option<bool>, reader)?;
        let input_verbosity = deserialize!(Option<RpcTransactionInputVerbosity>, reader)?;
        let output_verbosity = deserialize!(Option<RpcTransactionOutputVerbosity>, reader)?;
        let include_lock_time = load!(Option<bool>, reader)?;
        let include_subnetwork_id = load!(Option<bool>, reader)?;
        let include_gas = load!(Option<bool>, reader)?;
        let include_payload = load!(Option<bool>, reader)?;
        let include_mass = load!(Option<bool>, reader)?;
        let verbose_data_verbosity = deserialize!(Option<RpcTransactionVerboseDataVerbosity>, reader)?;

        Ok(Self {
            include_version,
            input_verbosity,
            output_verbosity,
            include_lock_time,
            include_subnetwork_id,
            include_gas,
            include_payload,
            include_mass,
            verbose_data_verbosity,
        })
    }
}

// RpcTransactionVerboseDataVerbosity
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RpcTransactionVerboseDataVerbosity {
    pub include_transaction_id: Option<bool>,
    pub include_hash: Option<bool>,
    pub include_compute_mass: Option<bool>,
    pub include_block_hash: Option<bool>,
    pub include_block_time: Option<bool>,
}

impl RpcTransactionVerboseDataVerbosity {
    pub fn new(
        include_transaction_id: Option<bool>,
        include_hash: Option<bool>,
        include_compute_mass: Option<bool>,
        include_block_hash: Option<bool>,
        include_block_time: Option<bool>,
    ) -> Self {
        Self { include_transaction_id, include_hash, include_compute_mass, include_block_hash, include_block_time }
    }
}

impl Serializer for RpcTransactionVerboseDataVerbosity {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(Option<bool>, &self.include_transaction_id, writer)?;
        store!(Option<bool>, &self.include_hash, writer)?;
        store!(Option<bool>, &self.include_compute_mass, writer)?;
        store!(Option<bool>, &self.include_block_hash, writer)?;
        store!(Option<bool>, &self.include_block_time, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcTransactionVerboseDataVerbosity {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;

        let include_transaction_id = load!(Option<bool>, reader)?;
        let include_hash = load!(Option<bool>, reader)?;
        let include_compute_mass = load!(Option<bool>, reader)?;
        let include_block_hash = load!(Option<bool>, reader)?;
        let include_block_time = load!(Option<bool>, reader)?;

        Ok(Self { include_transaction_id, include_hash, include_compute_mass, include_block_hash, include_block_time })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RpcAcceptanceDataVerbosity {
    pub accepting_chain_header_verbosity: Option<RpcHeaderVerbosity>,
    pub mergeset_block_acceptance_data_verbosity: Option<RpcMergesetBlockAcceptanceDataVerbosity>,
}

impl RpcAcceptanceDataVerbosity {
    pub fn new(
        accepting_chain_header_verbosity: Option<RpcHeaderVerbosity>,
        mergeset_block_acceptance_data_verbosity: Option<RpcMergesetBlockAcceptanceDataVerbosity>,
    ) -> Self {
        Self { accepting_chain_header_verbosity, mergeset_block_acceptance_data_verbosity }
    }

    pub fn requires_merged_header(&self, default: bool) -> bool {
        self.mergeset_block_acceptance_data_verbosity.as_ref().map_or(default, |active| active.requires_merged_header())
    }

    pub fn requeires_accepted_header(&self, default: bool) -> bool {
        self.mergeset_block_acceptance_data_verbosity.as_ref().map_or(default, |active| active.requires_merged_block_hash())
    }

    pub fn requires_accepted_transactions(&self, default: bool) -> bool {
        self.mergeset_block_acceptance_data_verbosity
            .as_ref()
            .map_or(default, |active| active.accepted_transactions_verbosity.is_some())
    }
}

impl Serializer for RpcAcceptanceDataVerbosity {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(Option<RpcHeaderVerbosity>, &self.accepting_chain_header_verbosity, writer)?;
        serialize!(Option<RpcMergesetBlockAcceptanceDataVerbosity>, &self.mergeset_block_acceptance_data_verbosity, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcAcceptanceDataVerbosity {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader);
        let accepting_chain_header_verbosity = load!(Option<RpcHeaderVerbosity>, reader)?;
        let mergeset_block_acceptance_data_verbosity = deserialize!(Option<RpcMergesetBlockAcceptanceDataVerbosity>, reader)?;

        Ok(Self { accepting_chain_header_verbosity, mergeset_block_acceptance_data_verbosity })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RpcMergesetBlockAcceptanceDataVerbosity {
    pub merged_header_verbosity: Option<RpcHeaderVerbosity>,
    pub accepted_transactions_verbosity: Option<RpcTransactionVerbosity>,
}

impl RpcMergesetBlockAcceptanceDataVerbosity {
    pub fn requires_merged_header(&self) -> bool {
        self.merged_header_verbosity.is_some()
            || self.accepted_transactions_verbosity.as_ref().is_some_and(|active| {
                active.verbose_data_verbosity.as_ref().is_some_and(|active| active.include_block_hash.unwrap_or(false))
            })
    }

    pub fn requires_merged_block_hash(&self) -> bool {
        self.merged_header_verbosity.as_ref().is_some_and(|active| active.include_hash.unwrap_or(false))
            || self.accepted_transactions_verbosity.as_ref().is_some_and(|active| {
                active.verbose_data_verbosity.as_ref().is_some_and(|active| active.include_block_hash.unwrap_or(false))
            })
    }
}

impl RpcMergesetBlockAcceptanceDataVerbosity {
    pub fn new(
        merged_header_verbosity: Option<RpcHeaderVerbosity>,
        accepted_transactions_verbosity: Option<RpcTransactionVerbosity>,
    ) -> Self {
        Self { merged_header_verbosity, accepted_transactions_verbosity }
    }
}

impl Serializer for RpcMergesetBlockAcceptanceDataVerbosity {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        serialize!(Option<RpcHeaderVerbosity>, &self.merged_header_verbosity, writer)?;
        serialize!(Option<RpcTransactionVerbosity>, &self.accepted_transactions_verbosity, writer)?;

        Ok(())
    }
}

impl Deserializer for RpcMergesetBlockAcceptanceDataVerbosity {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let merged_header_verbosity = deserialize!(Option<RpcHeaderVerbosity>, reader)?;
        let accepted_transactions_verbosity = deserialize!(Option<RpcTransactionVerbosity>, reader)?;

        Ok(Self { merged_header_verbosity, accepted_transactions_verbosity })
    }
}
