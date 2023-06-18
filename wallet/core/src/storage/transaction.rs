use faster_hex::hex_string;
use kaspa_utils::hex::ToHex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// pub type TransactionRecordId = u64;

// impl ToHex for TransactionRecordId {
//     fn to_hex(&self) -> String {
//         self.to_string()
//     }
// }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransactionRecordId(pub(crate) u64);

// impl TransactionRecordId {
//     pub(crate) fn new(id: u64) -> TransactionRecordId {
//         TransactionRecordId(id)
//     }
// }

impl ToHex for TransactionRecordId {
    fn to_hex(&self) -> String {
        format!("{:x}", self.0)
    }
}

impl Serialize for TransactionRecordId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex_string(&self.0.to_be_bytes()))
    }
}

impl<'de> Deserialize<'de> for TransactionRecordId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let hex_str = <std::string::String as Deserialize>::deserialize(deserializer)?;
        let mut out = [0u8; 8];
        let mut input = [b'0'; 16];
        let start = input.len() - hex_str.len();
        input[start..].copy_from_slice(hex_str.as_bytes());
        faster_hex::hex_decode(&input, &mut out).map_err(serde::de::Error::custom)?;
        Ok(TransactionRecordId(u64::from_be_bytes(out)))
    }
}

impl std::fmt::Display for TransactionRecordId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex_string(&self.0.to_be_bytes()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRecord {
    pub id: TransactionRecordId,
    // TODO
}
