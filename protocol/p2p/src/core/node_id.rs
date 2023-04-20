use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::{fmt::Display, ops::Deref, str::FromStr};
use uuid::Uuid;

#[derive(PartialEq, Eq, Hash, Copy, Clone, Serialize, Deserialize, Debug)]
#[repr(transparent)]
pub struct NodeId(pub Uuid);

impl NodeId {
    pub fn new(ip: Uuid) -> Self {
        Self(ip)
    }
}
impl From<Uuid> for NodeId {
    fn from(ip: Uuid) -> Self {
        Self(ip)
    }
}
impl From<NodeId> for Uuid {
    fn from(value: NodeId) -> Self {
        value.0
    }
}

impl FromStr for NodeId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::from_str(s).map(NodeId::from)
    }
}

impl Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl Deref for NodeId {
    type Target = Uuid;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

//
// Borsh serializers need to be manually implemented for `NodeId` since
// Uuid does not currently support Borsh
//

impl BorshSerialize for NodeId {
    fn serialize<W: borsh::maybestd::io::Write>(&self, writer: &mut W) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
        borsh::BorshSerialize::serialize(&self.0.as_bytes(), writer)?;
        Ok(())
    }
}

impl BorshDeserialize for NodeId {
    fn deserialize(buf: &mut &[u8]) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
        let bytes: uuid::Bytes = BorshDeserialize::deserialize(buf)?;
        Ok(Self::new(Uuid::from_bytes(bytes)))
    }
}

impl BorshSchema for NodeId {
    fn declaration() -> borsh::schema::Declaration {
        "NodeId".to_string()
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
    fn test_node_id_borsh() {
        // Tests for NodeId Borsh ser/deser since we manually implemented them
        let id: NodeId = Uuid::new_v4().into();
        let bin = id.try_to_vec().unwrap();
        let id2: NodeId = BorshDeserialize::try_from_slice(&bin).unwrap();
        assert_eq!(id, id2);

        let id: NodeId = Uuid::from_bytes([123u8; 16]).into();
        let bin = id.try_to_vec().unwrap();
        let id2: NodeId = BorshDeserialize::try_from_slice(&bin).unwrap();
        assert_eq!(id, id2);
    }
}
