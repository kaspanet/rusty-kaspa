use std::collections::BinaryHeap;

use kaspa_hashes::Hash;

use crate::{
    model::stores::{conflict_zone_coloring::ColoringData, headers::HeaderStoreReader},
    processes::{dagknight::umc_voting::ColoringReader, ghostdag::ordering::SortableBlock},
};

/// Lazy future-to-past iterator over the blue set of a colouring: the mergeset blues
/// of `coloring` itself and of every chain block along its selected-parent chain down
/// to `root`, the chain blocks, and `root` (yielded last).
///
/// Blocks are yielded by descending `SortableBlock` key (blue work, then hash), the
/// same order as sorting the collected set: blue work strictly increases along parent
/// edges, so every record sorts above its own mergeset blues. A chain record is read
/// only once its chain block is pulled, never ahead. The heap holds at most 2k+1
/// blocks at width k: expanding one record pushes at most k+1 mergeset blues, and at
/// most k blues of later chain records are still buffered, being concurrent with the
/// popped chain block. The stream's length equals the colouring's `blue_score`: each
/// chain record's score is its parent's score plus its mergeset-blues count, which
/// counts exactly the blocks streamed.
pub struct ReferenceIterator<'a, R: ColoringReader, H: HeaderStoreReader> {
    root: Hash,
    chain: Hash,
    heap: BinaryHeap<SortableBlock>,
    coloring_reader: &'a R,
    headers_store: &'a H,
}

impl<'a, R: ColoringReader, H: HeaderStoreReader> ReferenceIterator<'a, R, H> {
    /// Iterates the blue set of `coloring` future-to-past. `coloring.selected_parent`
    /// must start a selected-parent chain that reaches `root`.
    pub fn new(coloring: &ColoringData, root: Hash, coloring_reader: &'a R, headers_store: &'a H) -> Self {
        let mut it = Self {
            root,
            chain: coloring.selected_parent,
            heap: BinaryHeap::with_capacity(coloring.mergeset_blues.len() + 1),
            coloring_reader,
            headers_store,
        };
        for &blue in coloring.mergeset_blues.iter() {
            it.heap.push(it.sortable_block(blue));
        }
        it
    }

    fn sortable_block(&self, block: Hash) -> SortableBlock {
        SortableBlock::new(block, self.headers_store.get_header(block).unwrap().blue_work)
    }
}

impl<'a, R: ColoringReader, H: HeaderStoreReader> Iterator for ReferenceIterator<'a, R, H> {
    type Item = Hash;

    fn next(&mut self) -> Option<Hash> {
        let record = self.heap.pop()?;
        let block = record.hash;
        if block == self.chain && block != self.root {
            let data = self.coloring_reader.get_coloring_data(block);
            self.chain = data.selected_parent;
            for &blue in data.mergeset_blues.iter() {
                let blue = self.sortable_block(blue);
                debug_assert!(blue < record, "mergeset blues sort below their record");
                self.heap.push(blue);
            }
        }
        Some(block)
    }
}
