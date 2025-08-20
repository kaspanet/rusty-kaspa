mod builder;
pub use builder::*;

/// A holder for flattened byte data that can be sliced according to provided lengths.
///
/// This structure represents a flattened collection of byte slices that were
/// concatenated together, along with the original lengths of each slice.
/// It provides a cursor-like interface to iterate over the original slices
/// without allocating new memory.
pub struct FlattenedSliceHolder<'a> {
    /// The flattened byte data containing all slices concatenated together
    flattened_data: &'a [u8],
    /// The lengths of each original slice in the flattened data
    slice_lengths: &'a [u32],
}

impl<'a> FlattenedSliceHolder<'a> {
    /// Creates a new holder for flattened slice data.
    ///
    /// # Arguments
    /// * `flattened_data` - The concatenated byte data
    /// * `slice_lengths` - The lengths of each original slice
    ///
    /// # Example
    /// ```
    /// let data = b"helloworldrust";
    /// let lengths = [5, 5, 4]; // "hello", "world", "rust"
    /// let holder = kaspa_utils::flattened_slice::FlattenedSliceHolder::new(data, &lengths);
    /// ```
    pub fn new(flattened_data: &'a [u8], slice_lengths: &'a [u32]) -> Self {
        Self { flattened_data, slice_lengths }
    }

    /// Creates a new iterator over the slices.
    /// Each call creates a fresh iterator starting from the beginning.
    pub fn iter(&'a self) -> FlattenedSliceIterator<'a> {
        FlattenedSliceIterator::new(self)
    }

    /// Returns the number of non-empty slices that would be produced by iteration.
    /// Slices with length 0 are skipped and not counted.
    pub fn slice_count(&self) -> usize {
        self.slice_lengths.iter().filter(|&&len| len > 0).count()
    }

    /// Returns true if there are no slices to iterate over.
    pub fn is_empty(&self) -> bool {
        self.slice_lengths.is_empty() || self.flattened_data.is_empty()
    }

    /// Returns the total length of the flattened data.
    pub fn total_len(&self) -> usize {
        self.flattened_data.len()
    }

    /// Validates that the slice lengths match the flattened data length.
    /// Returns true if the sum of slice lengths equals the flattened data length.
    pub fn is_valid(&self) -> bool {
        let expected_len: u64 = self.slice_lengths.iter().map(|&len| len as u64).sum();
        expected_len == self.flattened_data.len() as u64
    }
}

/// An iterator over slices reconstructed from flattened data.
///
/// This iterator yields `&[u8]` slices by using the lengths to slice
/// the flattened data at appropriate boundaries.
pub struct FlattenedSliceIterator<'a> {
    holder: &'a FlattenedSliceHolder<'a>,
    current_slice_idx: usize,
    current_data_pos: usize,
}

impl<'a> FlattenedSliceIterator<'a> {
    fn new(holder: &'a FlattenedSliceHolder<'a>) -> Self {
        Self { holder, current_slice_idx: 0, current_data_pos: 0 }
    }
}

impl<'a> Iterator for FlattenedSliceIterator<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.current_slice_idx >= self.holder.slice_lengths.len() {
                return None;
            }

            let slice_length = self.holder.slice_lengths[self.current_slice_idx] as usize;

            // Skip empty slices (length 0) - advance position but don't yield
            if slice_length == 0 {
                self.current_slice_idx += 1;
                continue;
            }

            let end_pos = self.current_data_pos + slice_length;

            // Bounds check
            if end_pos > self.holder.flattened_data.len() {
                return None;
            }

            let slice = &self.holder.flattened_data[self.current_data_pos..end_pos];

            // Update position for next iteration
            self.current_data_pos = end_pos;
            self.current_slice_idx += 1;

            return Some(slice);
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        // Count remaining non-empty slices
        let remaining = self.holder.slice_lengths[self.current_slice_idx..].iter().filter(|&&len| len > 0).count();
        (remaining, Some(remaining))
    }
}

impl<'a> ExactSizeIterator for FlattenedSliceIterator<'a> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_functionality() {
        let data = b"helloworldrust";
        let lengths = [5u32, 5, 4]; // "hello", "world", "rust"
        let holder = FlattenedSliceHolder::new(data, &lengths);

        let slices: Vec<&[u8]> = holder.iter().collect();

        assert_eq!(slices.len(), 3);
        assert_eq!(slices[0], b"hello");
        assert_eq!(slices[1], b"world");
        assert_eq!(slices[2], b"rust");
    }

    #[test]
    fn test_multiple_iterations() {
        let data = b"abcdef";
        let lengths = [2u32, 2, 2]; // "ab", "cd", "ef"
        let holder = FlattenedSliceHolder::new(data, &lengths);

        // First iteration
        let first_iter: Vec<&[u8]> = holder.iter().collect();
        assert_eq!(first_iter, vec![b"ab", b"cd", b"ef"]);

        // Second iteration should work identically
        let second_iter: Vec<&[u8]> = holder.iter().collect();
        assert_eq!(second_iter, vec![b"ab", b"cd", b"ef"]);

        // Both should be equal
        assert_eq!(first_iter, second_iter);
    }

    #[test]
    fn test_empty_data() {
        let data = b"";
        let lengths: [u32; 0] = [];
        let holder = FlattenedSliceHolder::new(data, &lengths);

        assert!(holder.is_empty());
        assert_eq!(holder.slice_count(), 0);
        assert_eq!(holder.total_len(), 0);

        let slices: Vec<&[u8]> = holder.iter().collect();
        assert!(slices.is_empty());
    }

    #[test]
    fn test_single_slice() {
        let data = b"single";
        let lengths = [6u32];
        let holder = FlattenedSliceHolder::new(data, &lengths);

        let slices: Vec<&[u8]> = holder.iter().collect();
        assert_eq!(slices.len(), 1);
        assert_eq!(slices[0], b"single");
    }

    #[test]
    fn test_zero_length_slices_are_skipped() {
        let data = b"ab";
        let lengths = [1u32, 0, 1, 0]; // "a", "", "b", "" - empty slices should be skipped
        let holder = FlattenedSliceHolder::new(data, &lengths);

        let slices: Vec<&[u8]> = holder.iter().collect();
        assert_eq!(slices.len(), 2); // Only non-empty slices
        assert_eq!(slices[0], b"a");
        assert_eq!(slices[1], b"b");

        // slice_count should only count non-empty slices
        assert_eq!(holder.slice_count(), 2);
    }

    #[test]
    fn test_all_zero_length_slices() {
        let data = b"";
        let lengths = [0u32, 0, 0]; // All empty slices
        let holder = FlattenedSliceHolder::new(data, &lengths);

        let slices: Vec<&[u8]> = holder.iter().collect();
        assert!(slices.is_empty()); // No slices yielded
        assert_eq!(holder.slice_count(), 0);
    }

    #[test]
    fn test_validation() {
        // Valid case
        let data = b"abcdef";
        let lengths = [2u32, 2, 2];
        let holder = FlattenedSliceHolder::new(data, &lengths);
        assert!(holder.is_valid());

        // Invalid case - lengths sum is too large
        let lengths_invalid = [2u32, 2, 3]; // sums to 7, but data is 6 bytes
        let holder_invalid = FlattenedSliceHolder::new(data, &lengths_invalid);
        assert!(!holder_invalid.is_valid());

        // Invalid case - lengths sum is too small
        let lengths_short = [1u32, 1]; // sums to 2, but data is 6 bytes
        let holder_short = FlattenedSliceHolder::new(data, &lengths_short);
        assert!(!holder_short.is_valid());
    }

    #[test]
    fn test_iterator_bounds_checking() {
        // This should handle invalid length gracefully by stopping iteration
        let data = b"abc";
        let lengths = [2u32, 5]; // Second slice would overflow
        let holder = FlattenedSliceHolder::new(data, &lengths);

        let slices: Vec<&[u8]> = holder.iter().collect();
        assert_eq!(slices.len(), 1); // Should only get the first valid slice
        assert_eq!(slices[0], b"ab");
    }

    #[test]
    fn test_size_hint_with_empty_slices() {
        let data = b"abcdef";
        let lengths = [2u32, 0, 2, 0, 2]; // "ab", "", "cd", "", "ef" - 3 non-empty
        let holder = FlattenedSliceHolder::new(data, &lengths);

        let mut iter = holder.iter();
        assert_eq!(iter.size_hint(), (3, Some(3))); // Only non-empty slices counted

        iter.next(); // consume "ab"
        assert_eq!(iter.size_hint(), (2, Some(2)));

        iter.next(); // consume "cd" (skips the empty slice)
        assert_eq!(iter.size_hint(), (1, Some(1)));

        iter.next(); // consume "ef" (skips the empty slice)
        assert_eq!(iter.size_hint(), (0, Some(0)));
    }

    #[test]
    fn test_exact_size_iterator() {
        let data = b"123456";
        let lengths = [1u32, 0, 2, 3]; // "1", "", "23", "456" - 3 non-empty
        let holder = FlattenedSliceHolder::new(data, &lengths);

        let iter = holder.iter();
        assert_eq!(iter.len(), 3); // Only non-empty slices

        // Test that ExactSizeIterator works with standard library functions
        let collected: Vec<&[u8]> = holder.iter().collect();
        assert_eq!(collected.len(), holder.iter().len());
        assert_eq!(collected, vec![b"1".as_slice(), b"23", b"456"]);
    }

    #[test]
    fn test_real_world_usage() {
        // Simulate transaction payload prefixes scenario
        let tx_payload_prefixes_flattened = b"\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a";
        let tx_payload_prefixes_lengths = [3u32, 0, 2, 4, 0, 1]; // Include some empty slices

        let holder = FlattenedSliceHolder::new(tx_payload_prefixes_flattened.as_slice(), &tx_payload_prefixes_lengths);

        // Multiple iterations as mentioned in the original question
        for slice in holder.iter() {
            // First processing pass - empty slices are skipped
            println!("Processing slice of length: {}", slice.len());
            assert!(!slice.is_empty()); // No empty slices should be yielded
        }

        for slice in holder.iter() {
            // Second processing pass
            assert!(!slice.is_empty()); // All slices should be non-empty
        }

        let all_slices: Vec<&[u8]> = holder.iter().collect();
        assert_eq!(all_slices.len(), 4); // Only non-empty slices
        assert_eq!(all_slices[0], &[0x01, 0x02, 0x03]);
        assert_eq!(all_slices[1], &[0x04, 0x05]);
        assert_eq!(all_slices[2], &[0x06, 0x07, 0x08, 0x09]);
        assert_eq!(all_slices[3], &[0x0a]);
    }
}
