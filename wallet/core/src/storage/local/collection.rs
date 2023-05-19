use crate::imports::*;
use crate::result::Result;
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::hash::Hash;

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
}
