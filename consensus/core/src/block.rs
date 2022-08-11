use crate::header::Header;
use hashes::Hash;

pub struct Block {
    pub header: Header,
}

impl Block {
    pub fn new(hash: Hash, parents: Vec<Hash>) -> Self {
        Self { header: Header::new(hash, parents) }
    }

    pub fn from_header(header: Header) -> Self {
        Self { header }
    }
}
