use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::{fmt::Display, ops::Deref, str::FromStr};
use uuid::Uuid;

#[derive(PartialEq, Eq, Hash, Copy, Clone, Serialize, Deserialize, Debug, Default)]
#[repr(transparent)]
pub struct PeerId(pub Uuid);

impl PeerId {
    pub fn new(id: Uuid) -> Self {
        Self(id)
    }

    pub fn from_slice(bytes: &[u8]) -> Result<Self, uuid::Error> {
        Ok(Uuid::from_slice(bytes)?.into())
    }
}
impl From<Uuid> for PeerId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}
impl From<PeerId> for Uuid {
    fn from(value: PeerId) -> Self {
        value.0
    }
}

impl FromStr for PeerId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::from_str(s).map(PeerId::from)
    }
}

impl Display for PeerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Deref for PeerId {
    type Target = Uuid;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

//
// Borsh serializers need to be manually implemented for `PeerId` since
// Uuid does not currently support Borsh
//

impl BorshSerialize for PeerId {
    fn serialize<W: borsh::maybestd::io::Write>(&self, writer: &mut W) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
        borsh::BorshSerialize::serialize(&self.0.as_bytes(), writer)?;
        Ok(())
    }
}

impl BorshDeserialize for PeerId {
    fn deserialize(buf: &mut &[u8]) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
        let bytes: uuid::Bytes = BorshDeserialize::deserialize(buf)?;
        Ok(Self::new(Uuid::from_bytes(bytes)))
    }
}

impl BorshSchema for PeerId {
    fn declaration() -> borsh::schema::Declaration {
        "PeerId".to_string()
    }
    fn add_definitions_recursively(
        definitions: &mut borsh::maybestd::collections::HashMap<borsh::schema::Declaration, borsh::schema::Definition>,
    ) {
        let fields = borsh::schema::Fields::UnnamedFields(borsh::maybestd::vec![<uuid::Bytes>::declaration()]);
        let definition = borsh::schema::Definition::Struct { fields };
        Self::add_definition(Self::declaration(), definition, definitions);
        <uuid::Bytes>::add_definitions_recursively(definitions);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_id_borsh() {
        // Tests for PeerId Borsh ser/deser since we manually implemented them
        let id: PeerId = Uuid::new_v4().into();
        let bin = id.try_to_vec().unwrap();
        let id2: PeerId = BorshDeserialize::try_from_slice(&bin).unwrap();
        assert_eq!(id, id2);

        let id: PeerId = Uuid::from_bytes([123u8; 16]).into();
        let bin = id.try_to_vec().unwrap();
        let id2: PeerId = BorshDeserialize::try_from_slice(&bin).unwrap();
        assert_eq!(id, id2);
    }
}
