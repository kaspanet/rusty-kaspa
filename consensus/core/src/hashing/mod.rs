use hashes::Hasher;

pub mod tx;

trait HasherExtensions {
    fn write_len(&mut self, len: usize) -> &mut Self;
    fn write_var_bytes(&mut self, bytes: &[u8]) -> &mut Self;
    fn write_var_array<D: AsRef<[u8]>>(&mut self, arr: &[D]) -> &mut Self;
}

/// Fails at compile time if `usize::MAX > u64::MAX`.
/// If `usize` will ever grow larger than `u64`, we need to verify
/// that the lossy conversion below at `write_len` remains precise.
const _: usize = u64::MAX as usize - usize::MAX;

impl<T: Hasher> HasherExtensions for T {
    #[inline(always)]
    fn write_len(&mut self, len: usize) -> &mut Self {
        self.update((len as u64).to_le_bytes())
    }

    #[inline(always)]
    fn write_var_bytes(&mut self, bytes: &[u8]) -> &mut Self {
        self.write_len(bytes.len()).update(bytes)
    }

    #[inline(always)]
    fn write_var_array<D: AsRef<[u8]>>(&mut self, arr: &[D]) -> &mut Self {
        self.write_len(arr.len());
        for d in arr {
            self.update(d);
        }
        self
    }
}
