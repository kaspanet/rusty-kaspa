use super::FlattenedSliceHolder;

/// A builder for creating flattened slice collections.
///
/// This builder allows you to incrementally add byte slices, which are stored
/// in a flattened format internally. Once built, it can provide a
/// `FlattenedSliceHolder` for efficient iteration over the original slices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlattenedSliceBuilder {
    /// The flattened byte data containing all slices concatenated together
    flattened_data: Vec<u8>,
    /// The lengths of each slice in the flattened data
    slice_lengths: Vec<u32>,
}

impl FlattenedSliceBuilder {
    /// Creates a new empty builder.
    pub fn new() -> Self {
        Self { flattened_data: Vec::new(), slice_lengths: Vec::new() }
    }

    /// Creates a new builder with pre-allocated capacity.
    ///
    /// # Arguments
    /// * `data_capacity` - Expected total bytes to be stored
    /// * `slice_capacity` - Expected number of slices to be added
    pub fn with_capacity(data_capacity: usize, slice_capacity: usize) -> Self {
        Self { flattened_data: Vec::with_capacity(data_capacity), slice_lengths: Vec::with_capacity(slice_capacity) }
    }

    /// Adds a slice to the collection.
    ///
    /// # Arguments
    /// * `slice` - The byte slice to add to the collection
    ///
    /// # Example
    /// ```
    /// let mut builder = kaspa_utils::flattened_slice::FlattenedSliceBuilder::new();
    /// builder.add_slice(b"hello");
    /// builder.add_slice(b"world");
    /// ```
    pub fn add_slice(&mut self, slice: &[u8]) {
        // Only add non-empty slices to maintain the no-empty-slices invariant
        if !slice.is_empty() {
            self.flattened_data.extend_from_slice(slice);
            self.slice_lengths.push(slice.len() as u32);
        }
    }

    /// Adds multiple slices at once.
    ///
    /// # Arguments
    /// * `slices` - An iterator over byte slices to add
    ///
    /// # Example
    /// ```
    /// let mut builder = kaspa_utils::flattened_slice::FlattenedSliceBuilder::new();
    /// builder.add_slices([b"hello".as_slice(), b"world", b"rust"].iter().copied());
    /// ```
    pub fn add_slices<'a, I>(&mut self, slices: I)
    where
        I: IntoIterator<Item = &'a [u8]>,
    {
        for slice in slices {
            self.add_slice(slice);
        }
    }

    /// Returns the number of slices currently in the builder.
    pub fn slice_count(&self) -> usize {
        self.slice_lengths.len()
    }

    /// Returns the total number of bytes currently in the flattened data.
    pub fn total_len(&self) -> usize {
        self.flattened_data.len()
    }

    /// Returns true if no slices have been added.
    pub fn is_empty(&self) -> bool {
        self.slice_lengths.is_empty()
    }

    /// Returns the current capacity of the flattened data buffer.
    pub fn data_capacity(&self) -> usize {
        self.flattened_data.capacity()
    }

    /// Returns the current capacity of the slice lengths buffer.
    pub fn slice_capacity(&self) -> usize {
        self.slice_lengths.capacity()
    }

    /// Reserves additional capacity for flattened data.
    ///
    /// # Arguments
    /// * `additional` - Number of additional bytes to reserve
    pub fn reserve_data(&mut self, additional: usize) {
        self.flattened_data.reserve(additional);
    }

    /// Reserves additional capacity for slice count.
    ///
    /// # Arguments
    /// * `additional` - Number of additional slices to reserve capacity for
    pub fn reserve_slices(&mut self, additional: usize) {
        self.slice_lengths.reserve(additional);
    }

    /// Shrinks the capacity of both internal vectors to fit their current content.
    pub fn shrink_to_fit(&mut self) {
        self.flattened_data.shrink_to_fit();
        self.slice_lengths.shrink_to_fit();
    }

    /// Clears all slices from the builder, keeping the allocated capacity.
    pub fn clear(&mut self) {
        self.flattened_data.clear();
        self.slice_lengths.clear();
    }

    /// Creates a `FlattenedSliceHolder` that references this builder's data.
    ///
    /// The returned holder borrows from this builder, so the builder must
    /// remain alive while the holder is in use.
    ///
    /// # Example
    /// ```
    /// let mut builder = kaspa_utils::flattened_slice::FlattenedSliceBuilder::new();
    /// builder.add_slice(b"hello");
    /// builder.add_slice(b"world");
    ///
    /// let holder = builder.as_holder();
    /// for slice in holder.iter() {
    ///     println!("Slice: {:?}", slice);
    /// }
    /// ```
    pub fn as_holder(&self) -> FlattenedSliceHolder<'_> {
        FlattenedSliceHolder::new(&self.flattened_data, &self.slice_lengths)
    }

    pub fn into_inner(self) -> (Vec<u8>, Vec<u32>) {
        (self.flattened_data, self.slice_lengths)
    }

    /// Creates a builder from raw flattened data and slice lengths.
    pub fn from_raw(flattened_data: Vec<u8>, slice_lengths: Vec<u32>) -> Self {
        Self { flattened_data, slice_lengths }
    }

    /// Creates a builder from a `Vec<Vec<u8>>` of prefixes.
    pub fn from_prefixes(prefixes: Vec<Vec<u8>>) -> Self {
        Self::from_prefixes_ref(&prefixes)
    }

    /// Creates a builder from a slice of prefixes.
    pub fn from_prefixes_ref(prefixes: &[Vec<u8>]) -> Self {
        let mut builder = Self::new();
        for prefix in prefixes {
            builder.add_slice(prefix);
        }
        builder
    }

    /// Checks if any stored prefix matches the beginning of `payload`.
    pub fn contains_prefix(&self, payload: &[u8]) -> bool {
        self.as_holder().iter().any(|prefix| payload.starts_with(prefix))
    }

    /// Returns the number of slices.
    pub fn len(&self) -> usize {
        self.as_holder().slice_count()
    }

    /// Returns a reference to the flattened byte data.
    pub fn flattened_data(&self) -> &[u8] {
        &self.flattened_data
    }

    /// Returns a reference to the slice lengths.
    pub fn slice_lengths(&self) -> &[u32] {
        &self.slice_lengths
    }
}

impl Default for FlattenedSliceBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> FromIterator<&'a [u8]> for FlattenedSliceBuilder {
    fn from_iter<T: IntoIterator<Item = &'a [u8]>>(iter: T) -> Self {
        let mut builder = Self::new();
        builder.add_slices(iter);
        builder
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_basic_functionality() {
        let mut builder = FlattenedSliceBuilder::new();

        // Initially empty
        assert!(builder.is_empty());
        assert_eq!(builder.slice_count(), 0);
        assert_eq!(builder.total_len(), 0);

        // Add some slices
        builder.add_slice(b"hello");
        builder.add_slice(b"world");
        builder.add_slice(b"rust");

        assert!(!builder.is_empty());
        assert_eq!(builder.slice_count(), 3);
        assert_eq!(builder.total_len(), 14); // 5 + 5 + 4

        // Test the holder
        let holder = builder.as_holder();
        let slices: Vec<&[u8]> = holder.iter().collect();

        assert_eq!(slices, vec![b"hello".as_slice(), b"world", b"rust"]);
    }

    #[test]
    fn test_builder_with_capacity() {
        let builder = FlattenedSliceBuilder::with_capacity(100, 10);

        assert_eq!(builder.data_capacity(), 100);
        assert_eq!(builder.slice_capacity(), 10);
        assert!(builder.is_empty());
    }

    #[test]
    fn test_builder_add_slices() {
        let mut builder = FlattenedSliceBuilder::new();

        let slices = [b"one".as_slice(), b"two", b"three"];
        builder.add_slices(slices.iter().copied());

        assert_eq!(builder.slice_count(), 3);

        let holder = builder.as_holder();
        let result: Vec<&[u8]> = holder.iter().collect();
        assert_eq!(result, vec![b"one".as_slice(), b"two", b"three"]);
    }

    #[test]
    fn test_builder_skips_empty_slices() {
        let mut builder = FlattenedSliceBuilder::new();

        builder.add_slice(b"hello");
        builder.add_slice(b""); // Empty slice - should be skipped
        builder.add_slice(b"world");
        builder.add_slice(b""); // Another empty slice - should be skipped

        // Should only have 2 slices, empty ones were skipped
        assert_eq!(builder.slice_count(), 2);
        assert_eq!(builder.total_len(), 10); // "hello" + "world"

        let holder = builder.as_holder();
        let slices: Vec<&[u8]> = holder.iter().collect();
        assert_eq!(slices, vec![b"hello", b"world"]);
    }

    #[test]
    fn test_builder_capacity_management() {
        let mut builder = FlattenedSliceBuilder::new();

        // Test reserve
        builder.reserve_data(1000);
        builder.reserve_slices(50);

        assert!(builder.data_capacity() >= 1000);
        assert!(builder.slice_capacity() >= 50);

        // Add some data
        builder.add_slice(b"test");

        let data_cap_before = builder.data_capacity();
        let slice_cap_before = builder.slice_capacity();

        // Shrink to fit
        builder.shrink_to_fit();

        assert!(builder.data_capacity() <= data_cap_before);
        assert!(builder.slice_capacity() <= slice_cap_before);
        assert_eq!(builder.total_len(), 4); // Still has the data
    }

    #[test]
    fn test_builder_clear() {
        let mut builder = FlattenedSliceBuilder::new();

        builder.add_slice(b"hello");
        builder.add_slice(b"world");

        assert_eq!(builder.slice_count(), 2);
        assert_eq!(builder.total_len(), 10);

        builder.clear();

        assert!(builder.is_empty());
        assert_eq!(builder.slice_count(), 0);
        assert_eq!(builder.total_len(), 0);

        // Capacity should be preserved
        assert!(builder.data_capacity() > 0);
        assert!(builder.slice_capacity() > 0);
    }

    #[test]
    fn test_builder_multiple_holders() {
        let mut builder = FlattenedSliceBuilder::new();
        builder.add_slice(b"test");
        builder.add_slice(b"data");

        // Multiple holders can be created from the same builder
        let holder1 = builder.as_holder();
        let holder2 = builder.as_holder();

        let slices1: Vec<&[u8]> = holder1.iter().collect();
        let slices2: Vec<&[u8]> = holder2.iter().collect();

        assert_eq!(slices1, slices2);
        assert_eq!(slices1, vec![b"test", b"data"]);
    }

    #[test]
    fn test_builder_default() {
        let builder = FlattenedSliceBuilder::default();
        assert!(builder.is_empty());
        assert_eq!(builder.slice_count(), 0);
        assert_eq!(builder.total_len(), 0);
    }

    #[test]
    fn test_builder_real_world_usage() {
        let mut builder = FlattenedSliceBuilder::with_capacity(1024, 100);

        // Simulate building transaction payload prefixes
        let payloads = [b"\x01\x02\x03".as_slice(), b"\x04\x05".as_slice(), b"\x06\x07\x08\x09".as_slice(), b"\x0a".as_slice()];

        for payload in payloads {
            builder.add_slice(payload);
        }

        assert_eq!(builder.slice_count(), 4);
        assert_eq!(builder.total_len(), 10);

        // Create holder and iterate multiple times
        let holder = builder.as_holder();

        // First pass
        let mut count = 0;
        for slice in holder.iter() {
            count += 1;
            assert!(!slice.is_empty());
        }
        assert_eq!(count, 4);

        // Second pass
        let all_slices: Vec<&[u8]> = holder.iter().collect();
        assert_eq!(all_slices.len(), 4);
        assert_eq!(all_slices[0], &[0x01, 0x02, 0x03]);
        assert_eq!(all_slices[1], &[0x04, 0x05]);
        assert_eq!(all_slices[2], &[0x06, 0x07, 0x08, 0x09]);
        assert_eq!(all_slices[3], &[0x0a]);
    }
}
