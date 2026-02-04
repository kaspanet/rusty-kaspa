use bytemuck::Zeroable;
use risc0_zkvm::serde::WordRead;
use zk_covenant_rollup_core::{state::AccountWitness, PublicInput};

/// Read a single u32 from stdin
pub fn read_u32(stdin: &mut impl WordRead) -> u32 {
    let mut value = 0u32;
    stdin.read_words(core::slice::from_mut(&mut value)).unwrap();
    value
}

/// Read a u64 from stdin (as 2 words)
pub fn read_u64(stdin: &mut impl WordRead) -> u64 {
    let mut words = [0u32; 2];
    stdin.read_words(&mut words).unwrap();
    u64::from_le_bytes(bytemuck::cast(words))
}

/// Read a hash ([u32; 8]) from stdin
pub fn read_hash(stdin: &mut impl WordRead) -> [u32; 8] {
    let mut hash = [0u32; 8];
    stdin.read_words(&mut hash).unwrap();
    hash
}

/// Read public input from stdin
pub fn read_public_input(stdin: &mut impl WordRead) -> PublicInput {
    let mut public_input = PublicInput::zeroed();
    stdin.read_words(public_input.as_words_mut()).unwrap();
    public_input
}

/// Read account witness from stdin
pub fn read_account_witness(stdin: &mut impl WordRead) -> AccountWitness {
    let mut witness = AccountWitness::zeroed();
    stdin.read_words(bytemuck::cast_slice_mut(bytemuck::bytes_of_mut(&mut witness))).unwrap();
    witness
}

/// Read variable-length bytes from stdin (length-prefixed as u64)
pub fn read_bytes(stdin: &mut impl WordRead) -> alloc::vec::Vec<u8> {
    use alloc::vec;
    use alloc::vec::Vec;

    let len = read_u64(stdin) as usize;
    if len == 0 {
        return Vec::new();
    }

    // Read as words, padding if necessary
    let num_words = (len + 3) / 4;
    let mut words = vec![0u32; num_words];
    stdin.read_words(&mut words).unwrap();

    // Convert to bytes and truncate to actual length
    let bytes: &[u8] = bytemuck::cast_slice(&words);
    bytes[..len].to_vec()
}
