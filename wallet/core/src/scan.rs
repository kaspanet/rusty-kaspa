use crate::address::AddressManager;
use crate::Address;
use crate::Result;
use std::sync::Arc;

pub struct Cursor {
    pub done: bool,
    index: u32,
    derivation: Arc<AddressManager>,
}

impl Cursor {
    pub fn new(derivation: Arc<AddressManager>) -> Self {
        Self { index: 0, done: false, derivation }
    }

    pub async fn next(&mut self, n: u32) -> Result<Vec<Address>> {
        let list = self.derivation.get_range(self.index..self.index + n).await?;
        self.index += n;
        Ok(list)
    }
}

pub enum ScanExtent {
    /// Scan until an empty range is found
    EmptyRange(u32),
    /// Scan until a specific depth (a particular derivation index)
    Depth(u32),
}

pub struct Scan {
    pub derivations: Vec<Cursor>,
    pub window_size: u32,
    pub extent: ScanExtent,
    pub pos: usize,
}

impl Scan {
    pub fn new(receive: Arc<AddressManager>, change: Arc<AddressManager>, window_size: u32, extent: ScanExtent) -> Self {
        let derivations = vec![Cursor::new(receive), Cursor::new(change)];
        Scan { derivations, window_size, extent, pos: 0 }
    }

    pub async fn next(&mut self) -> Result<Option<Vec<Address>>> {
        let len = self.derivations.len();
        if let Some(cursor) = self.derivations.get_mut(self.pos) {
            self.pos += 1;
            if self.pos >= len {
                self.pos = 0;
            }

            let list = cursor.next(self.window_size).await?;
            Ok(Some(list))
        } else {
            Ok(None)
        }
    }
}
