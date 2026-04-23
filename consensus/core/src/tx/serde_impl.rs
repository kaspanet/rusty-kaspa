//! Version-aware serde for [`Transaction`].
//!
//! The outer shape is stable across versions — 9 fields in a fixed order:
//! `version`, `inputs`, `outputs`, `lockTime`, `subnetworkId`, `gas`,
//! `payload`, `mass`, `id`. What depends on `version` is the per-element
//! shape inside `inputs` and `outputs`:
//!
//! - v0 inputs carry a flat `sigOpCount: u8`; v0 outputs have no `covenant`
//!   field at all.
//! - v1+ inputs carry a flat `computeBudget: u16`; v1+ outputs carry a
//!   trailing `covenant: Option<CovenantBinding>`.
//!
//! Both JSON and bincode go through this single implementation. JSON uses the
//! camelCase field names produced by the helper structs below; bincode uses
//! the positional layout those same structs derive. The `Ref*` structs drive
//! serialization (borrowing from a live `Transaction`) and the `*Owned`
//! structs drive deserialization; because every pair is declared with an
//! identical field order and the `SigopCount` / `ComputeBudget` newtypes are
//! `#[serde(transparent)]` over `u8` / `u16`, the two sides share the exact
//! same wire format by construction.

use super::{CovenantBinding, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, TxInputMass};
use crate::mass::{ComputeBudget, SigopCount};
use kaspa_utils::serde_bytes;
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, DeserializeSeed, IntoDeserializer, MapAccess, SeqAccess, Visitor},
    ser::{SerializeSeq, SerializeStruct},
};
use serde_json::value::RawValue as BufferedValue;

#[repr(transparent)]
struct SerdeBytesRef<'a>(&'a [u8]);

impl Serialize for SerdeBytesRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serde_bytes::serialize(&self.0, serializer)
    }
}

#[derive(Deserialize)]
#[repr(transparent)]
struct SerdeBytesOwned(#[serde(with = "serde_bytes")] Vec<u8>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TxInputV0Ref<'a> {
    previous_outpoint: &'a TransactionOutpoint,
    #[serde(with = "serde_bytes")]
    signature_script: &'a [u8],
    sequence: u64,
    sig_op_count: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TxInputV1Ref<'a> {
    previous_outpoint: &'a TransactionOutpoint,
    #[serde(with = "serde_bytes")]
    signature_script: &'a [u8],
    sequence: u64,
    compute_budget: u16,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TxOutputV0Ref<'a> {
    value: u64,
    script_public_key: &'a ScriptPublicKey,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TxOutputV1Ref<'a> {
    value: u64,
    script_public_key: &'a ScriptPublicKey,
    covenant: &'a Option<CovenantBinding>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TxInputV0Owned {
    previous_outpoint: TransactionOutpoint,
    #[serde(with = "serde_bytes")]
    signature_script: Vec<u8>,
    sequence: u64,
    sig_op_count: SigopCount,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TxInputV1Owned {
    previous_outpoint: TransactionOutpoint,
    #[serde(with = "serde_bytes")]
    signature_script: Vec<u8>,
    sequence: u64,
    compute_budget: ComputeBudget,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TxOutputV0Owned {
    value: u64,
    script_public_key: ScriptPublicKey,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TxOutputV1Owned {
    value: u64,
    script_public_key: ScriptPublicKey,
    #[serde(default)]
    covenant: Option<CovenantBinding>,
}

impl From<TxInputV0Owned> for TransactionInput {
    fn from(value: TxInputV0Owned) -> Self {
        Self {
            previous_outpoint: value.previous_outpoint,
            signature_script: value.signature_script,
            sequence: value.sequence,
            mass: TxInputMass::SigopCount(value.sig_op_count),
        }
    }
}

impl From<TxInputV1Owned> for TransactionInput {
    fn from(value: TxInputV1Owned) -> Self {
        Self {
            previous_outpoint: value.previous_outpoint,
            signature_script: value.signature_script,
            sequence: value.sequence,
            mass: TxInputMass::ComputeBudget(value.compute_budget),
        }
    }
}

impl From<TxOutputV0Owned> for TransactionOutput {
    fn from(value: TxOutputV0Owned) -> Self {
        Self { value: value.value, script_public_key: value.script_public_key, covenant: None }
    }
}

impl From<TxOutputV1Owned> for TransactionOutput {
    fn from(value: TxOutputV1Owned) -> Self {
        Self { value: value.value, script_public_key: value.script_public_key, covenant: value.covenant }
    }
}

// -------------------------------------------------------------------------
// Serialize
// -------------------------------------------------------------------------

/// Tiny wrapper that picks the right per-element helper based on `version`
/// when serializing the `inputs` sequence.
struct InputsRef<'a> {
    version: u16,
    inputs: &'a [TransactionInput],
}

impl Serialize for InputsRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(self.inputs.len()))?;
        match self.version {
            0 => {
                for input in self.inputs {
                    seq.serialize_element(&TxInputV0Ref {
                        previous_outpoint: &input.previous_outpoint,
                        signature_script: input.signature_script.as_slice(),
                        sequence: input.sequence,
                        sig_op_count: input.mass.sig_op_count().expect("v0 transaction inputs must carry a SigopCount mass"),
                    })?;
                }
            }
            _ => {
                for input in self.inputs {
                    seq.serialize_element(&TxInputV1Ref {
                        previous_outpoint: &input.previous_outpoint,
                        signature_script: input.signature_script.as_slice(),
                        sequence: input.sequence,
                        compute_budget: input.mass.compute_budget().expect("v1+ transaction inputs must carry a ComputeBudget mass"),
                    })?;
                }
            }
        }
        seq.end()
    }
}

/// Counterpart of [`InputsRef`] for the `outputs` sequence.
struct OutputsRef<'a> {
    version: u16,
    outputs: &'a [TransactionOutput],
}

impl Serialize for OutputsRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(self.outputs.len()))?;
        match self.version {
            0 => {
                for output in self.outputs {
                    seq.serialize_element(&TxOutputV0Ref { value: output.value, script_public_key: &output.script_public_key })?;
                }
            }
            _ => {
                for output in self.outputs {
                    seq.serialize_element(&TxOutputV1Ref {
                        value: output.value,
                        script_public_key: &output.script_public_key,
                        covenant: &output.covenant,
                    })?;
                }
            }
        }
        seq.end()
    }
}

impl Serialize for Transaction {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("Transaction", 9)?;
        state.serialize_field("version", &self.version)?;
        state.serialize_field("inputs", &InputsRef { version: self.version, inputs: &self.inputs })?;
        state.serialize_field("outputs", &OutputsRef { version: self.version, outputs: &self.outputs })?;
        state.serialize_field("lockTime", &self.lock_time)?;
        state.serialize_field("subnetworkId", &self.subnetwork_id)?;
        state.serialize_field("gas", &self.gas)?;
        state.serialize_field("payload", &SerdeBytesRef(&self.payload))?;
        state.serialize_field("mass", &self.mass)?;
        state.serialize_field("id", &self.id)?;
        state.end()
    }
}

/// Field discriminator for `visit_map`. Declared at module scope (rather than
/// inside the deserializer body) so `visit_seq` / `visit_map` stay compact.
enum Field {
    Version,
    Inputs,
    Outputs,
    LockTime,
    SubnetworkId,
    Gas,
    Payload,
    Mass,
    Id,
}

impl<'de> Deserialize<'de> for Field {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "camelCase")]
        enum FieldInner {
            Version,
            Inputs,
            Outputs,
            LockTime,
            SubnetworkId,
            Gas,
            Payload,
            Mass,
            Id,
        }

        Ok(match FieldInner::deserialize(deserializer)? {
            FieldInner::Version => Field::Version,
            FieldInner::Inputs => Field::Inputs,
            FieldInner::Outputs => Field::Outputs,
            FieldInner::LockTime => Field::LockTime,
            FieldInner::SubnetworkId => Field::SubnetworkId,
            FieldInner::Gas => Field::Gas,
            FieldInner::Payload => Field::Payload,
            FieldInner::Mass => Field::Mass,
            FieldInner::Id => Field::Id,
        })
    }
}

/// Threads the transaction `version` into the per-input deserialization so
/// the right V0/V1 Owned helper is chosen based on the already-decoded
/// `version` field.
struct InputsSeed(u16);

impl<'de> DeserializeSeed<'de> for InputsSeed {
    type Value = Vec<TransactionInput>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        match self.0 {
            0 => Vec::<TxInputV0Owned>::deserialize(deserializer).map(|inputs| inputs.into_iter().map(Into::into).collect()),
            _ => Vec::<TxInputV1Owned>::deserialize(deserializer).map(|inputs| inputs.into_iter().map(Into::into).collect()),
        }
    }
}

/// Counterpart of [`InputsSeed`] for outputs.
struct OutputsSeed(u16);

impl<'de> DeserializeSeed<'de> for OutputsSeed {
    type Value = Vec<TransactionOutput>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        match self.0 {
            0 => Vec::<TxOutputV0Owned>::deserialize(deserializer).map(|outputs| outputs.into_iter().map(Into::into).collect()),
            _ => Vec::<TxOutputV1Owned>::deserialize(deserializer).map(|outputs| outputs.into_iter().map(Into::into).collect()),
        }
    }
}

struct TransactionVisitor;

impl<'de> Visitor<'de> for TransactionVisitor {
    type Value = Transaction;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("struct Transaction")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let version: u16 = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(0, &self))?;
        let inputs = seq.next_element_seed(InputsSeed(version))?.ok_or_else(|| de::Error::invalid_length(1, &self))?;
        let outputs = seq.next_element_seed(OutputsSeed(version))?.ok_or_else(|| de::Error::invalid_length(2, &self))?;
        let lock_time = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(3, &self))?;
        let subnetwork_id = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(4, &self))?;
        let gas = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(5, &self))?;
        let payload = seq.next_element::<SerdeBytesOwned>()?.ok_or_else(|| de::Error::invalid_length(6, &self))?.0;
        // `mass` keeps its historical `#[serde(default)]` leniency; `id` is required and
        // read verbatim — serde does not recompute the txid on deserialization.
        let mass = seq.next_element()?.unwrap_or_default();
        let id = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(8, &self))?;

        Ok(Transaction { version, inputs, outputs, lock_time, subnetwork_id, gas, payload, mass, id })
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        // `inputs` / `outputs` depend on `version` to pick the right V0/V1 shape, but
        // human-readable formats do not promise field order. When those fields arrive
        // before `version`, buffer them as [`BufferedValue`] and decode after the loop
        // once the version is known.
        enum LazyInputs {
            Decoded(Vec<TransactionInput>),
            Buffered(Box<BufferedValue>),
        }
        enum LazyOutputs {
            Decoded(Vec<TransactionOutput>),
            Buffered(Box<BufferedValue>),
        }

        let mut version = None;
        let mut inputs: Option<LazyInputs> = None;
        let mut outputs: Option<LazyOutputs> = None;
        let mut lock_time = None;
        let mut subnetwork_id = None;
        let mut gas = None;
        let mut payload = None;
        let mut mass = None;
        let mut id = None;

        while let Some(field) = map.next_key()? {
            match field {
                Field::Version => {
                    if version.is_some() {
                        return Err(de::Error::duplicate_field("version"));
                    }
                    version = Some(map.next_value()?);
                }
                Field::Inputs => {
                    if inputs.is_some() {
                        return Err(de::Error::duplicate_field("inputs"));
                    }
                    inputs = Some(match version {
                        Some(v) => LazyInputs::Decoded(map.next_value_seed(InputsSeed(v))?),
                        None => LazyInputs::Buffered(map.next_value()?),
                    });
                }
                Field::Outputs => {
                    if outputs.is_some() {
                        return Err(de::Error::duplicate_field("outputs"));
                    }
                    outputs = Some(match version {
                        Some(v) => LazyOutputs::Decoded(map.next_value_seed(OutputsSeed(v))?),
                        None => LazyOutputs::Buffered(map.next_value()?),
                    });
                }
                Field::LockTime => {
                    if lock_time.is_some() {
                        return Err(de::Error::duplicate_field("lockTime"));
                    }
                    lock_time = Some(map.next_value()?);
                }
                Field::SubnetworkId => {
                    if subnetwork_id.is_some() {
                        return Err(de::Error::duplicate_field("subnetworkId"));
                    }
                    subnetwork_id = Some(map.next_value()?);
                }
                Field::Gas => {
                    if gas.is_some() {
                        return Err(de::Error::duplicate_field("gas"));
                    }
                    gas = Some(map.next_value()?);
                }
                Field::Payload => {
                    if payload.is_some() {
                        return Err(de::Error::duplicate_field("payload"));
                    }
                    payload = Some(map.next_value::<SerdeBytesOwned>()?.0);
                }
                Field::Mass => {
                    if mass.is_some() {
                        return Err(de::Error::duplicate_field("mass"));
                    }
                    mass = Some(map.next_value()?);
                }
                Field::Id => {
                    if id.is_some() {
                        return Err(de::Error::duplicate_field("id"));
                    }
                    id = Some(map.next_value()?);
                }
            }
        }

        let version = version.ok_or_else(|| de::Error::missing_field("version"))?;
        let inputs = match inputs.ok_or_else(|| de::Error::missing_field("inputs"))? {
            LazyInputs::Decoded(v) => v,
            LazyInputs::Buffered(buf) => InputsSeed(version).deserialize(buf.into_deserializer()).map_err(de::Error::custom)?,
        };
        let outputs = match outputs.ok_or_else(|| de::Error::missing_field("outputs"))? {
            LazyOutputs::Decoded(v) => v,
            LazyOutputs::Buffered(buf) => OutputsSeed(version).deserialize(buf.into_deserializer()).map_err(de::Error::custom)?,
        };
        let lock_time = lock_time.ok_or_else(|| de::Error::missing_field("lockTime"))?;
        let subnetwork_id = subnetwork_id.ok_or_else(|| de::Error::missing_field("subnetworkId"))?;
        let gas = gas.ok_or_else(|| de::Error::missing_field("gas"))?;
        let payload = payload.ok_or_else(|| de::Error::missing_field("payload"))?;

        let id = id.ok_or_else(|| de::Error::missing_field("id"))?;

        // `mass` keeps its historical `#[serde(default)]` leniency; `id` is required and
        // read verbatim — serde does not recompute the txid on deserialization.
        Ok(Transaction { version, inputs, outputs, lock_time, subnetwork_id, gas, payload, mass: mass.unwrap_or_default(), id })
    }
}

impl<'de> Deserialize<'de> for Transaction {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        const FIELDS: &[&str] = &["version", "inputs", "outputs", "lockTime", "subnetworkId", "gas", "payload", "mass", "id"];
        deserializer.deserialize_struct("Transaction", FIELDS, TransactionVisitor)
    }
}
