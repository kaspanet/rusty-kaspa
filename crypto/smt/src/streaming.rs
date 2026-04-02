//! Streaming SMT builder for sorted leaves.
//!
//! Stack-machine that processes sorted leaves one at a time, computing hashes
//! and writing nodes inline via a [`MergeSink`]. Bounded memory: the stack
//! never exceeds 256 entries.

use alloc::vec::Vec;
use core::fmt;
use core::marker::PhantomData;

use kaspa_hashes::Hash;

use crate::store::{BranchKey, CollapsedLeaf};
use crate::{DEPTH, SmtHasher, hash_node};

/// Errors that can occur during streaming SMT construction.
#[derive(Debug)]
pub enum StreamError<E: fmt::Debug> {
    UnsortedKey,
    DuplicateKey,
    AlreadyFinalized,
    NotFinalized,
    Sink(E),
}

impl<E: fmt::Debug> fmt::Display for StreamError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsortedKey => write!(f, "key is not strictly greater than the previous key"),
            Self::DuplicateKey => write!(f, "duplicate key"),
            Self::AlreadyFinalized => write!(f, "feed() called after finalization"),
            Self::NotFinalized => write!(f, "finish() called before all leaves were fed"),
            Self::Sink(e) => write!(f, "sink error: {e:?}"),
        }
    }
}

/// Describes a child node in a merge operation (for the sink to persist).
#[derive(Clone, Copy)]
pub enum ChildInfo {
    Empty,
    Collapsed { branch_key: BranchKey, leaf: CollapsedLeaf },
    Internal,
}

/// Callback interface for persisting SMT nodes produced by [`StreamingSmtBuilder`].
pub trait MergeSink {
    type Error: fmt::Debug;

    /// Merge two sibling subtrees into a parent node. Returns the parent hash.
    fn merge(
        &mut self,
        left: Hash,
        right: Hash,
        parent_key: BranchKey,
        left_info: ChildInfo,
        right_info: ChildInfo,
    ) -> Result<Hash, Self::Error>;

    /// Chain a subtree upward through empty siblings, from `from_depth` to `to_depth`.
    fn merge_chain_with_empty(
        &mut self,
        hash: Hash,
        from_depth: usize,
        to_depth: usize,
        representative_key: &Hash,
    ) -> Result<Hash, Self::Error>;

    /// Write a collapsed single-leaf subtree at the given position.
    fn write_collapsed(&mut self, branch_key: BranchKey, leaf: CollapsedLeaf) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EntryKind {
    Collapsed(CollapsedLeaf),
    Internal,
}

#[derive(Copy, Clone)]
struct StackEntry {
    depth: usize,
    hash: Hash,
    representative_key: Hash,
    kind: EntryKind,
}

impl StackEntry {
    fn child_info(&self, child_depth: u8) -> ChildInfo {
        match self.kind {
            EntryKind::Collapsed(leaf) => {
                ChildInfo::Collapsed { branch_key: BranchKey::new(child_depth, &self.representative_key), leaf }
            }
            EntryKind::Internal => ChildInfo::Internal,
        }
    }
}

/// Streaming SMT builder that processes sorted leaves one at a time.
///
/// Maintains a bounded stack (max depth 256). Each [`feed`](Self::feed) call
/// triggers merges for completed subtrees and writes nodes via the [`MergeSink`].
/// After all leaves are fed, call [`finish`](Self::finish) to get the root hash.
pub struct StreamingSmtBuilder<H: SmtHasher, S: MergeSink> {
    stack: Vec<StackEntry>,
    current: Option<StackEntry>,
    remaining: u64,
    /// `None` while feeding, `Some` once finalized.
    root: Option<Hash>,
    root_written: bool,
    sink: S,
    _phantom: PhantomData<H>,
}

impl<H: SmtHasher, S: MergeSink> StreamingSmtBuilder<H, S> {
    pub fn new(total_count: u64, sink: S) -> Self {
        let root = if total_count == 0 { Some(H::empty_root()) } else { None };
        Self {
            stack: Vec::with_capacity(64),
            current: None,
            remaining: total_count,
            root,
            root_written: false,
            sink,
            _phantom: PhantomData,
        }
    }

    /// Feed the next leaf (must be in strictly ascending key order).
    /// Automatically finalizes when `total_count` leaves have been fed.
    pub fn feed(&mut self, lane_key: Hash, leaf_hash: Hash) -> Result<(), StreamError<S::Error>> {
        if self.root.is_some() {
            return Err(StreamError::AlreadyFinalized);
        }

        if let Some(mut prev) = self.current.take() {
            if lane_key <= prev.representative_key {
                let err = if lane_key == prev.representative_key { StreamError::DuplicateKey } else { StreamError::UnsortedKey };
                self.current = Some(prev);
                return Err(err);
            }
            let div_depth = divergence_depth(&prev.representative_key, &lane_key);
            self.seal_up_to(&mut prev, div_depth)?;
            self.stack.push(prev);
        }

        self.remaining -= 1;

        let collapsed = CollapsedLeaf { lane_key, leaf_hash };
        let collapsed_hash = hash_node::<H::CollapsedHasher>(lane_key, leaf_hash);

        self.current = Some(StackEntry {
            depth: DEPTH,
            hash: collapsed_hash,
            representative_key: lane_key,
            kind: EntryKind::Collapsed(collapsed),
        });

        if self.remaining == 0 {
            self.finalize()?;
        }

        Ok(())
    }

    /// Consume the builder, returning the root hash and the sink.
    pub fn finish(self) -> Result<(Hash, S), StreamError<S::Error>> {
        match self.root {
            Some(root) => Ok((root, self.sink)),
            None => Err(StreamError::NotFinalized),
        }
    }

    pub fn sink(&self) -> &S {
        &self.sink
    }

    fn seal_up_to(&mut self, current: &mut StackEntry, target_depth: usize) -> Result<(), StreamError<S::Error>> {
        while let Some(&StackEntry { depth: top_depth, .. }) = self.stack.last()
            && top_depth >= target_depth
        {
            if current.depth > top_depth {
                self.chain_up(current, top_depth)?;
            }
            let left = self.stack.pop().unwrap(); // safe: just peeked
            self.merge_pair(current, left)?;
        }

        if current.depth > target_depth {
            self.chain_up(current, target_depth)?;
        }
        Ok(())
    }

    /// Chain `current` upward to `target_depth` by merging with empty siblings.
    /// Collapsed entries just update depth; internal entries write intermediate nodes.
    fn chain_up(&mut self, current: &mut StackEntry, target_depth: usize) -> Result<(), StreamError<S::Error>> {
        if current.depth <= target_depth {
            return Ok(());
        }
        match current.kind {
            EntryKind::Collapsed(_) => current.depth = target_depth,
            EntryKind::Internal => {
                let chain_to = target_depth + 1;
                if current.depth > chain_to {
                    current.hash = self
                        .sink
                        .merge_chain_with_empty(current.hash, current.depth, chain_to, &current.representative_key)
                        .map_err(StreamError::Sink)?;
                }
                current.depth = target_depth;
            }
        }
        Ok(())
    }

    fn merge_pair(&mut self, current: &mut StackEntry, left: StackEntry) -> Result<(), StreamError<S::Error>> {
        let merge_depth = left.depth;
        let parent_depth = merge_depth as u8;
        let parent_key = BranchKey::new(parent_depth, &left.representative_key);

        let (left_info, right_info) = if parent_depth < (DEPTH - 1) as u8 {
            let child_depth = parent_depth + 1;
            (left.child_info(child_depth), current.child_info(child_depth))
        } else {
            (ChildInfo::Internal, ChildInfo::Internal)
        };

        let result = self.sink.merge(left.hash, current.hash, parent_key, left_info, right_info).map_err(StreamError::Sink)?;

        if merge_depth == 0 {
            self.root_written = true;
        }

        current.depth = merge_depth;
        current.hash = result;
        current.representative_key = left.representative_key;
        current.kind = EntryKind::Internal;

        Ok(())
    }

    fn finalize(&mut self) -> Result<(), StreamError<S::Error>> {
        let Some(mut current) = self.current.take() else {
            self.root = Some(H::empty_root());
            return Ok(());
        };

        self.seal_up_to(&mut current, 0)?;
        debug_assert_eq!(current.depth, 0);

        match current.kind {
            EntryKind::Collapsed(cl) => {
                let bk = BranchKey::new(0, &current.representative_key);
                self.sink.write_collapsed(bk, cl).map_err(StreamError::Sink)?;
                self.root = Some(hash_node::<H::CollapsedHasher>(cl.lane_key, cl.leaf_hash));
            }
            EntryKind::Internal => {
                if self.root_written {
                    self.root = Some(current.hash);
                } else {
                    let result = self
                        .sink
                        .merge_chain_with_empty(current.hash, 1, 0, &current.representative_key)
                        .map_err(StreamError::Sink)?;
                    self.root = Some(result);
                }
            }
        }

        Ok(())
    }
}

/// Bit index where two keys first differ (0 = MSB of first byte, 256 = identical).
pub fn divergence_depth(a: &Hash, b: &Hash) -> usize {
    let a = a.as_slice();
    let b = b.as_slice();
    for i in 0..32 {
        let xor = a[i] ^ b[i];
        if xor != 0 {
            return i * 8 + xor.leading_zeros() as usize;
        }
    }
    256
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bit_at;
    use crate::store::{BTreeSmtStore, LeafUpdate, SortedLeafUpdates};
    use crate::tree::compute_root_update;
    use kaspa_hashes::SeqCommitActiveNode;

    pub struct InlineMergeSink<H: SmtHasher> {
        pub nodes: Vec<(BranchKey, crate::store::Node)>,
        _phantom: PhantomData<H>,
    }

    impl<H: SmtHasher> InlineMergeSink<H> {
        pub fn new() -> Self {
            Self { nodes: Vec::new(), _phantom: PhantomData }
        }
    }

    impl<H: SmtHasher> Default for InlineMergeSink<H> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<H: SmtHasher> MergeSink for InlineMergeSink<H> {
        type Error = core::convert::Infallible;

        fn merge(
            &mut self,
            left: Hash,
            right: Hash,
            parent_key: BranchKey,
            left_info: ChildInfo,
            right_info: ChildInfo,
        ) -> Result<Hash, Self::Error> {
            use crate::store::Node;
            if let ChildInfo::Collapsed { branch_key, leaf } = left_info {
                self.nodes.push((branch_key, Node::Collapsed(leaf)));
            }
            if let ChildInfo::Collapsed { branch_key, leaf } = right_info {
                self.nodes.push((branch_key, Node::Collapsed(leaf)));
            }

            let parent_hash = hash_node::<H>(left, right);
            self.nodes.push((parent_key, Node::Internal(parent_hash)));
            Ok(parent_hash)
        }

        fn merge_chain_with_empty(
            &mut self,
            hash: Hash,
            from_depth: usize,
            to_depth: usize,
            representative_key: &Hash,
        ) -> Result<Hash, Self::Error> {
            use crate::store::Node;
            let mut current_hash = hash;

            for d in (to_depth..from_depth).rev() {
                let height = DEPTH - 1 - d;
                let goes_right = bit_at(representative_key, d);
                let empty_h = H::EMPTY_HASHES[height];
                let (left_h, right_h) = if goes_right { (empty_h, current_hash) } else { (current_hash, empty_h) };
                current_hash = hash_node::<H>(left_h, right_h);
                let bk = BranchKey::new(d as u8, representative_key);
                self.nodes.push((bk, Node::Internal(current_hash)));
            }

            Ok(current_hash)
        }

        fn write_collapsed(&mut self, branch_key: BranchKey, leaf: CollapsedLeaf) -> Result<(), Self::Error> {
            self.nodes.push((branch_key, crate::store::Node::Collapsed(leaf)));
            Ok(())
        }
    }

    type H = SeqCommitActiveNode;

    fn check(leaves: &[(Hash, Hash)]) {
        let normalized = SortedLeafUpdates::from_unsorted(leaves.iter().map(|(k, h)| LeafUpdate { key: *k, leaf_hash: *h }));
        let store = BTreeSmtStore::new();
        let (expected_root, expected_changes) = compute_root_update::<H, _>(&store, H::empty_root(), normalized).unwrap();

        let normalized = SortedLeafUpdates::from_unsorted(leaves.iter().map(|(k, h)| LeafUpdate { key: *k, leaf_hash: *h }));
        let sink = InlineMergeSink::<H>::new();
        let mut builder = StreamingSmtBuilder::<H, _>::new(normalized.len() as u64, sink);
        for u in normalized.iter() {
            builder.feed(u.key, u.leaf_hash).unwrap();
        }
        let (root, sink) = builder.finish().unwrap();

        assert_eq!(root, expected_root);
        assert_eq!(sink.nodes.len(), expected_changes.values().filter(|n| n.is_some()).count());
    }

    #[test]
    fn one_leaf() {
        check(&[(Hash::from_bytes([1; 32]), Hash::from_bytes([2; 32]))]);
    }

    #[test]
    fn two_leaves() {
        check(&[(Hash::from_bytes([1; 32]), Hash::from_bytes([2; 32])), (Hash::from_bytes([3; 32]), Hash::from_bytes([4; 32]))]);
    }

    #[test]
    fn three_leaves() {
        check(&[
            (Hash::from_bytes([1; 32]), Hash::from_bytes([2; 32])),
            (Hash::from_bytes([3; 32]), Hash::from_bytes([4; 32])),
            (Hash::from_bytes([5; 32]), Hash::from_bytes([6; 32])),
        ]);
    }

    #[test]
    fn four_leaves() {
        check(&[
            (Hash::from_bytes([1; 32]), Hash::from_bytes([10; 32])),
            (Hash::from_bytes([2; 32]), Hash::from_bytes([20; 32])),
            (Hash::from_bytes([3; 32]), Hash::from_bytes([30; 32])),
            (Hash::from_bytes([4; 32]), Hash::from_bytes([40; 32])),
        ]);
    }

    #[test]
    fn empty_tree() {
        let sink = InlineMergeSink::<H>::new();
        let builder = StreamingSmtBuilder::<H, _>::new(0, sink);
        let (root, sink) = builder.finish().unwrap();
        assert_eq!(root, H::empty_root());
        assert!(sink.nodes.is_empty());
    }

    #[test]
    fn adjacent_leaves_last_bit_differs() {
        let k1 = [0u8; 32];
        let mut k2 = [0u8; 32];
        k2[31] = 1;
        check(&[(Hash::from_bytes(k1), Hash::from_bytes([0xAA; 32])), (Hash::from_bytes(k2), Hash::from_bytes([0xBB; 32]))]);
    }

    #[test]
    fn leaves_differ_at_root_bit() {
        let k1 = Hash::from_bytes([0x00; 32]);
        let mut k2_bytes = [0x00; 32];
        k2_bytes[0] = 0x80;
        let k2 = Hash::from_bytes(k2_bytes);
        check(&[(k1, Hash::from_bytes([0xAA; 32])), (k2, Hash::from_bytes([0xBB; 32]))]);
    }

    #[test]
    fn randomized_small() {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        for count in [2, 3, 5, 7, 10, 16, 32, 50] {
            let leaves: Vec<(Hash, Hash)> = (0..count)
                .map(|_| {
                    let mut k = [0u8; 32];
                    let mut v = [0u8; 32];
                    rng.fill(&mut k);
                    rng.fill(&mut v);
                    (Hash::from_bytes(k), Hash::from_bytes(v))
                })
                .collect();
            check(&leaves);
        }
    }

    #[test]
    fn randomized_large() {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        for count in [100, 500, 1000] {
            let leaves: Vec<(Hash, Hash)> = (0..count)
                .map(|_| {
                    let mut k = [0u8; 32];
                    let mut v = [0u8; 32];
                    rng.fill(&mut k);
                    rng.fill(&mut v);
                    (Hash::from_bytes(k), Hash::from_bytes(v))
                })
                .collect();
            check(&leaves);
        }
    }

    #[test]
    fn error_unsorted() {
        let k1 = Hash::from_bytes([2; 32]);
        let k2 = Hash::from_bytes([1; 32]);
        let mut builder = StreamingSmtBuilder::<H, _>::new(2, InlineMergeSink::<H>::new());
        builder.feed(k1, Hash::from_bytes([0xAA; 32])).unwrap();
        assert!(matches!(builder.feed(k2, Hash::from_bytes([0xBB; 32])), Err(StreamError::UnsortedKey)));
    }

    #[test]
    fn error_duplicate() {
        let k = Hash::from_bytes([1; 32]);
        let mut builder = StreamingSmtBuilder::<H, _>::new(2, InlineMergeSink::<H>::new());
        builder.feed(k, Hash::from_bytes([0xAA; 32])).unwrap();
        assert!(matches!(builder.feed(k, Hash::from_bytes([0xBB; 32])), Err(StreamError::DuplicateKey)));
    }

    #[test]
    fn error_feed_after_finalized() {
        let mut builder = StreamingSmtBuilder::<H, _>::new(1, InlineMergeSink::<H>::new());
        builder.feed(Hash::from_bytes([1; 32]), Hash::from_bytes([2; 32])).unwrap();
        assert!(matches!(builder.feed(Hash::from_bytes([3; 32]), Hash::from_bytes([4; 32])), Err(StreamError::AlreadyFinalized)));
    }

    #[test]
    fn error_not_finalized() {
        let mut builder = StreamingSmtBuilder::<H, _>::new(2, InlineMergeSink::<H>::new());
        builder.feed(Hash::from_bytes([1; 32]), Hash::from_bytes([2; 32])).unwrap();
        assert!(matches!(builder.finish(), Err(StreamError::NotFinalized)));
    }
}
