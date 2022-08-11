use hashes::Hash;

pub struct Header {
    pub hash: Hash, // TEMP: until consensushashing is ready
    pub parents: Vec<Hash>,
}

impl Header {
    pub fn new(hash: Hash, parents: Vec<Hash>) -> Self {
        Self { hash, parents }
    }
}
