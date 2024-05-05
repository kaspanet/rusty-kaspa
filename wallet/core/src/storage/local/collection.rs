//!
//! Ordered collections used to store wallet primitives.
//!

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
        if self.map.contains_key(&id) {
            self.map.remove(&id);
            self.vec.retain(|d| d.id() != &id);
        }

        self.map.insert(id, data.clone());
        self.vec.push(data);
        Ok(())
    }

    pub fn extend(&mut self, list: &[(Id, Data)]) -> Result<()> {
        let ids = list.iter().map(|(id, _)| id).collect::<Vec<_>>();
        self.remove(&ids)?;

        list.iter().for_each(|(id, data)| {
            let data = Arc::new((*data).clone());
            self.map.insert(id.clone(), data.clone());
            self.vec.push(data);
        });

        Ok(())
    }

    pub fn remove(&mut self, ids: &[&Id]) -> Result<()> {
        self.vec.retain(|data| {
            let id = data.id();
            if ids.contains(&id) {
                self.map.remove(id);
                false
            } else {
                true
            }
        });

        Ok(())
    }

    pub fn store_multiple(&mut self, data: Vec<Data>) -> Result<()> {
        for data in data.into_iter() {
            let id = data.id().clone();
            if self.map.contains_key(&id) {
                self.map.remove(&id);
                self.vec.retain(|d| d.id() != &id);
            }

            let data = Arc::new(data);
            self.map.insert(id.clone(), data.clone());
            self.vec.push(data);
        }
        Ok(())
    }

    pub fn store_single(&mut self, data: &Data) -> Result<()> {
        let id = data.id();
        if self.map.contains_key(id) {
            self.map.remove(id);
            self.vec.retain(|d| d.id() != id);
        }

        let data = Arc::new((*data).clone());
        self.map.insert(id.clone(), data.clone());
        self.vec.push(data);
        Ok(())
    }

    pub fn load_single(&self, id: &Id) -> Result<Option<Arc<Data>>> {
        Ok(self.map.get(id).cloned())
    }

    pub fn load_multiple(&self, ids: &[Id]) -> Result<Vec<Arc<Data>>> {
        ids.iter()
            .map(|id| match self.map.get(id).cloned() {
                Some(data) => Ok(data),
                None => Err(Error::KeyId(id.to_string())),
            })
            .collect::<Result<Vec<_>>>()
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
