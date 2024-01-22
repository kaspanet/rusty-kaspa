use kaspa_consensus_core::subnets::SUBNETWORK_ID_SIZE;

// Defualts for txindex perf params:
pub const DEFAULT_TXINDEX_MEMORY_BUDGET: usize = 1_000_000_000; // 1 GB
pub const DEFAULT_TXINDEX_EXTRA_FD_BUDGET: usize = 0;
pub const DEFAULT_TXINDEX_DB_PARALLELISM: usize = 1;

// We define these as constants for a default network transaction here to be able to calculate the approx. expected size & mass of a std transaction.

/// The approximate size with schnorr as the most common script public key.
pub const SCHNORR_SCRIPT_PUBLIC_KEY_BYTES_PER_TRANSACTION: u64 = (34u64) * 3u64; // we expect 1 input and 2 outputs in most cases

/// The approx size of a default standard network transaction in bytes.
pub const DEFAULT_TRANSACTION_SIZE: u64 = {
    // Header:
    2u64 // transaction version
    + 8u64 // number of inputs
    + 8u64 // number of outputs 
    + 4u64 // lock time
    + SUBNETWORK_ID_SIZE as u64 // subnetwork id size
    + 8u64 // gas
    + 32u64 // payload hash
    + 8u64 // payload length

    // Inputs:
    +  32u64 // previous transaction id
    + 4u64 // previous transaction index
    + 8u64 // value
    + 1u64 // signature script length
    + 4u64 // sequence
    + 34u64 // script public key len
    + 1 + 64 + 1 // signature -> 1 byte for OP_DATA_65 + 64 (length of signature) + 1 byte for sig hash type

    // Outputs:
    + (
        8u64 // value
        + 8u64 // script length
        + 34u64 // script public key len
        + 4u64 // script version
        + 8u64 // sequence
    ) * 2u64 // we expect 2 outputs in most cases, with the change address.
};

/// The approx amount of a default standard network transaction Sig OPs per input.
pub const DEFAULT_TRANSACTION_SIG_OPS: u64 = 1u64; // input OP_CHECKSIG
