use crate::tx::{Transaction, TransactionInput, TransactionOutpoint, TransactionOutput};
use blake2b_simd::{Hash, State};

/// Not intended for direct use by clients. Instead use `tx.id()`
pub fn id(tx: &Transaction) -> Hash {
    let mut hasher = blake2b_simd::Params::new().hash_length(32).key(b"TransactionID").to_state();
    write_transaction_for_transaction_id(&mut hasher, tx);
    hasher.finalize()
}

/// Write the transaction into the provided hasher according to the encoding flags
fn write_transaction_for_transaction_id(hasher: &mut State, tx: &Transaction) {
    hasher.update(&tx.version.to_le_bytes()).write_len(tx.inputs.len());
    for input in tx.inputs.iter() {
        // Write the tx input
        write_input(hasher, input);
    }

    hasher.write_len(tx.outputs.len());
    for output in tx.outputs.iter() {
        // Write the tx output
        write_output(hasher, output);
    }

    hasher.update(&tx.lock_time.to_le_bytes()).update(&tx.subnetwork_id.0).update(&tx.gas.to_le_bytes()).write_var_bytes(&tx.payload);

    /*
       Design principles (mostly related to the new mass commitment field; see KIP-0009):
           1. The new mass field should not modify tx::id (since it is essentially a commitment by the miner re block space usage
              so there is no need to modify the id definition which will require wide-spread changes in ecosystem software).
           2. Coinbase tx hash should ideally remain unchanged

       Solution:
           1. Hash the mass field only for tx::hash
           2. Hash the mass field only if mass > 0
           3. Require in consensus that coinbase mass == 0

       This way we have:
           - Unique commitment for tx::hash per any possible mass value (with only zero being a no-op)
           - tx::id remains unmodified
           - Coinbase tx hash remains unchanged
    */
}

#[inline(always)]
fn write_input(hasher: &mut State, input: &TransactionInput) {
    write_outpoint(hasher, &input.previous_outpoint);
    hasher.write_var_bytes(&[]);
    hasher.update(&input.sequence.to_le_bytes());
}

#[inline(always)]
fn write_outpoint(hasher: &mut State, outpoint: &TransactionOutpoint) {
    hasher.update(outpoint.transaction_id.as_bytes()).update(&outpoint.index.to_le_bytes());
}

#[inline(always)]
fn write_output(hasher: &mut State, output: &TransactionOutput) {
    hasher
        .update(&output.value.to_le_bytes())
        .update(&output.script_public_key.version().to_le_bytes())
        .write_var_bytes(output.script_public_key.script());
}

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

    /// Writes the number of bytes followed by the bytes themselves
    fn write_var_bytes(&mut self, bytes: &[u8]) -> &mut Self;

    /// Writes the array len followed by each element as [[u8]]
    fn write_var_array<D: AsRef<[u8]>>(&mut self, arr: &[D]) -> &mut Self;
}

/// Fails at compile time if `usize::MAX > u64::MAX`.
/// If `usize` will ever grow larger than `u64`, we need to verify
/// that the lossy conversion below at `write_len` remains precise.
const _: usize = u64::MAX as usize - usize::MAX;

impl HasherExtensions for State {
    #[inline(always)]
    fn write_len(&mut self, len: usize) -> &mut Self {
        self.update(&(len as u64).to_le_bytes())
    }

    #[inline(always)]
    fn write_bool(&mut self, element: bool) -> &mut Self {
        self.update(if element { &[1u8] } else { &[0u8] })
    }

    fn write_u8(&mut self, element: u8) -> &mut Self {
        self.update(&element.to_le_bytes())
    }

    fn write_u16(&mut self, element: u16) -> &mut Self {
        self.update(&element.to_le_bytes())
    }

    #[inline(always)]
    fn write_u32(&mut self, element: u32) -> &mut Self {
        self.update(&element.to_le_bytes())
    }

    #[inline(always)]
    fn write_u64(&mut self, element: u64) -> &mut Self {
        self.update(&element.to_le_bytes())
    }

    #[inline(always)]
    fn write_var_bytes(&mut self, bytes: &[u8]) -> &mut Self {
        self.write_len(bytes.len()).update(bytes)
    }

    #[inline(always)]
    fn write_var_array<D: AsRef<[u8]>>(&mut self, arr: &[D]) -> &mut Self {
        self.write_len(arr.len());
        for d in arr {
            self.update(d.as_ref());
        }
        self
    }
}
