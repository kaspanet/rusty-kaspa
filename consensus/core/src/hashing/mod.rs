use crate::BlueWorkType;
use kaspa_hashes::HasherBase;

pub mod header;
pub mod sighash;
pub mod sighash_type;
pub mod tx;
#[cfg(feature = "wasm32-sdk")]
pub mod wasm;

pub trait HasherExtensions {
    /// Writes the len as u64 little endian bytes
    fn write_len(&mut self, len: usize) -> &mut Self;

    /// Writes the boolean as a u8  
    fn write_bool(&mut self, element: bool) -> &mut Self;

    /// Writes a single u8  
    fn write_u8(&mut self, element: u8) -> &mut Self;

    /// Writes the u16 as a little endian u8 array  
    fn write_u16(&mut self, element: u16) -> &mut Self;

    /// Writes the u32 as a little endian u8 array  
    fn write_u32(&mut self, element: u32) -> &mut Self;

    /// Writes the u64 as a little endian u8 array  
    fn write_u64(&mut self, element: u64) -> &mut Self;

    /// Writes blue work as big endian bytes w/o the leading zeros
    /// (emulates bigint.bytes() in the kaspad golang ref)
    fn write_blue_work(&mut self, work: BlueWorkType) -> &mut Self;

    /// Writes the number of bytes followed by the bytes themselves
    fn write_var_bytes(&mut self, bytes: &[u8]) -> &mut Self;

    /// Writes the array len followed by each element as [[u8]]
    fn write_var_array<D: AsRef<[u8]>>(&mut self, arr: &[D]) -> &mut Self;
}

/// Fails at compile time if `usize::MAX > u64::MAX`.
/// If `usize` will ever grow larger than `u64`, we need to verify
/// that the lossy conversion below at `write_len` remains precise.
const _: usize = u64::MAX as usize - usize::MAX;

impl<T: HasherBase> HasherExtensions for T {
    #[inline(always)]
    fn write_len(&mut self, len: usize) -> &mut Self {
        self.update((len as u64).to_le_bytes())
    }

    #[inline(always)]
    fn write_bool(&mut self, element: bool) -> &mut Self {
        self.update(if element { [1u8] } else { [0u8] })
    }

    fn write_u8(&mut self, element: u8) -> &mut Self {
        self.update(element.to_le_bytes())
    }

    fn write_u16(&mut self, element: u16) -> &mut Self {
        self.update(element.to_le_bytes())
    }

    #[inline(always)]
    fn write_u32(&mut self, element: u32) -> &mut Self {
        self.update(element.to_le_bytes())
    }

    #[inline(always)]
    fn write_u64(&mut self, element: u64) -> &mut Self {
        self.update(element.to_le_bytes())
    }

    #[inline(always)]
    fn write_blue_work(&mut self, work: BlueWorkType) -> &mut Self {
        let be_bytes = work.to_be_bytes();
        let start = be_bytes.iter().copied().position(|byte| byte != 0).unwrap_or(be_bytes.len());

        self.write_var_bytes(&be_bytes[start..])
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
