use crate::error::Error;
use crate::result::Result;
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::hash::Hash;
use std::sync::Arc;

use crate::storage::IdT;

pub struct Collection<Id, Data>
where
    Id: std::hash::Hash + std::cmp::Eq,
    Data: IdT,
{
    pub vec: Vec<Arc<Data>>,
    pub map: HashMap<Id, Arc<Data>>,
}

impl<Id, Data> Default for Collection<Id, Data>
where
    Id: std::hash::Hash + std::cmp::Eq,
    Data: IdT,
{
    fn default() -> Self {
        Self { vec: Vec::new(), map: HashMap::new() }
    }
}

impl<Id, Data> Collection<Id, Data>
where
    Id: Clone + Hash + Eq + Display + Debug + Send,
    Data: Clone + IdT<Id = Id>,
{
    pub fn len(&self) -> usize {
        self.vec.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vec.is_empty()
    }

    pub fn insert(&mut self, id: Id, data: Arc<Data>) -> Result<()> {
        if self.map.get(&id).is_some() {
            self.map.remove(&id);
            self.vec.retain(|d| d.id() != &id);
        }

        self.map.insert(id, data.clone());
        self.vec.push(data);
        Ok(())
    }

    pub fn remove_item(&mut self, id: &Id) -> Option<Arc<Data>> {
        if let Some(data) = self.map.remove(id) {
            self.vec.retain(|d| d.id() != id);
            Some(data)
        } else {
            None
        }
    }

    pub fn remove(&mut self, ids: &[Id]) -> Result<()> {
        self.vec.retain(|data| {
            let id = data.id();
            if ids.contains(id) {
                self.map.remove(id);
                false
            } else {
                true
            }
        });

        Ok(())
    }

    pub fn store(&mut self, data: &[&Data]) -> Result<()> {
        for data in data.iter() {
            let id = data.id();
            if self.map.get(id).is_some() {
                self.map.remove(id);
                self.vec.retain(|d| d.id() != id);
            }

            let data = Arc::new((*data).clone());
            self.map.insert(id.clone(), data.clone());
            self.vec.push(data.clone());
        }
        Ok(())
    }

    pub fn load(&self, ids: &[Id]) -> Result<Vec<Arc<Data>>> {
        Ok(ids
            .iter()
            .filter_map(|id| match self.map.get(id).cloned() {
                Some(data) => Some(data),
                None => panic!("requested id `{}` was not found in collection", id),
            })
            .collect())
    }

    pub fn range(&self, range: std::ops::Range<usize>) -> Result<Vec<Arc<Data>>> {
        Ok(self.vec[range.start..range.end].to_vec())
    }
}

impl<Id, Data> TryFrom<Vec<Data>> for Collection<Id, Data>
where
    Id: Copy + std::hash::Hash + std::cmp::Eq,
    Data: IdT<Id = Id>,
{
    type Error = Error;

    fn try_from(vec: Vec<Data>) -> Result<Self> {
        let vec = vec.into_iter().map(|data| Arc::new(data)).collect::<Vec<_>>();
        let map = vec.iter().map(|data| (*data.id(), data.clone())).collect::<HashMap<_, _>>();

        Ok(Self { vec, map })
    }
}

impl<Id, Data> TryFrom<&Collection<Id, Data>> for Vec<Data>
where
    Id: Clone + std::hash::Hash + std::cmp::Eq,
    Data: Clone + IdT<Id = Id>,
{
    type Error = Error;

    fn try_from(collection: &Collection<Id, Data>) -> Result<Self> {
        Ok(collection.vec.iter().map(|data| (**data).clone()).collect::<Vec<_>>())
    }
}

// impl Serialize for Hash {
//     fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
//     where
//         S: Serializer,
//     {
//         if serializer.is_human_readable() {
//             let mut hex = [0u8; HASH_SIZE * 2];
//             faster_hex::hex_encode(&self.0, &mut hex).expect("The output is exactly twice the size of the input");
//             serializer.serialize_str(str::from_utf8(&hex).expect("hex is always valid UTF-8"))
//         } else {
//             serializer.serialize_bytes(&self.0)
//         }
//     }
// }

// impl<'de> Deserialize<'de> for Hash {
//     fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//     where
//         D: Deserializer<'de>,
//     {
//         if deserializer.is_human_readable() {
//             let s = <std::string::String as Deserialize>::deserialize(deserializer)?;
//             FromStr::from_str(&s).map_err(serde::de::Error::custom)
//         } else {
//             let s = <Vec<u8> as Deserialize>::deserialize(deserializer)?;
//             Ok(Self::try_from_slice(&s).map_err(D::Error::custom)?)
//         }
//     }
// }
