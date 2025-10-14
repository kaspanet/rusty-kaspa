use crate::{
    RpcAcceptanceDataVerbosity, RpcDataVerbosityLevel, RpcError, RpcHeaderVerbosity, RpcMergesetBlockAcceptanceDataVerbosity,
    RpcTransactionInputVerboseDataVerbosity, RpcTransactionInputVerbosity, RpcTransactionOutputVerboseDataVerbosity,
    RpcTransactionOutputVerbosity, RpcTransactionVerboseDataVerbosity, RpcTransactionVerbosity, RpcUtxoEntryVerboseDataVerbosity,
    RpcUtxoEntryVerbosity,
};

macro_rules! impl_verbosity_from {
    (
        for $target:ty, from $level:ty {
            $( $field:ident : $handler:tt ),* $(,)?
        }
    ) => {
        impl ::core::convert::From<$level> for $target {
            fn from(level: $level) -> Self {
                Self {
                    $(
                        $field: impl_verbosity_from!(@eval level, $level, $handler),
                    )*
                }
            }
        }
    };

    // (|level| expr) -> Some(expr_result)
    (@eval $lev:ident, $levty:ty, (| $L:ident | $e:expr)) => {
        ::core::option::Option::Some((|$L: $levty| { $e })($lev))
    };

    // (Level) -> Some(bool)
    (@eval $lev:ident, $levty:ty, ($min_level:expr)) => {
        ::core::option::Option::Some($lev.is_at_least($min_level))
    };

    // (none) -> None
    (@eval $lev:ident, $levty:ty, (none)) => {
        ::core::option::Option::None
    };
}

impl RpcDataVerbosityLevel {
    /// Check if this verbosity level is at least the specified level
    pub const fn is_at_least(&self, other: Self) -> bool {
        *self as u8 >= other as u8
    }
}

impl From<RpcDataVerbosityLevel> for i32 {
    fn from(v: RpcDataVerbosityLevel) -> Self {
        v as i32
    }
}

impl TryFrom<i32> for RpcDataVerbosityLevel {
    type Error = RpcError;
    fn try_from(v: i32) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::None),
            1 => Ok(Self::Low),
            2 => Ok(Self::High),
            3 => Ok(Self::Full),
            _ => Err(RpcError::NotImplemented),
        }
    }
}

impl_verbosity_from! {
    for RpcHeaderVerbosity, from RpcDataVerbosityLevel {
        include_hash:                    (RpcDataVerbosityLevel::None),
        include_version:                 (RpcDataVerbosityLevel::Low),
        include_timestamp:               (RpcDataVerbosityLevel::Low),
        include_bits:                    (RpcDataVerbosityLevel::Low),
        include_nonce:                   (RpcDataVerbosityLevel::Low),
        include_daa_score:               (RpcDataVerbosityLevel::Low),
        include_blue_work:               (RpcDataVerbosityLevel::Low),
        include_blue_score:              (RpcDataVerbosityLevel::Low),
        include_parents_by_level:        (RpcDataVerbosityLevel::High),
        include_hash_merkle_root:        (RpcDataVerbosityLevel::High),
        include_accepted_id_merkle_root: (RpcDataVerbosityLevel::High),
        include_utxo_commitment:         (RpcDataVerbosityLevel::Full),
        include_pruning_point:           (RpcDataVerbosityLevel::Full),
    }
}

impl_verbosity_from! {
    for RpcUtxoEntryVerboseDataVerbosity, from RpcDataVerbosityLevel {
        include_script_public_key_type:      (RpcDataVerbosityLevel::Low),
        include_script_public_key_address:   (RpcDataVerbosityLevel::Low),
    }
}

impl_verbosity_from! {
    for RpcUtxoEntryVerbosity, from RpcDataVerbosityLevel {
        include_amount:            (RpcDataVerbosityLevel::High),
        include_script_public_key: (RpcDataVerbosityLevel::High),
        include_block_daa_score:   (RpcDataVerbosityLevel::Full),
        include_is_coinbase:       (RpcDataVerbosityLevel::High),
        verbose_data_verbosity:    (|level| {
            RpcUtxoEntryVerboseDataVerbosity::from(level)
        }),
    }
}

impl_verbosity_from! {
    for RpcTransactionInputVerbosity, from RpcDataVerbosityLevel {
        include_signature_script:   (RpcDataVerbosityLevel::Low),
        include_sequence:           (RpcDataVerbosityLevel::High),
        include_sig_op_count:       (RpcDataVerbosityLevel::High),
        include_previous_outpoint:  (RpcDataVerbosityLevel::High),
        verbose_data_verbosity:     (|level| {
            RpcTransactionInputVerboseDataVerbosity::from(level)
        }),

    }
}

impl_verbosity_from! {
    for RpcTransactionInputVerboseDataVerbosity, from RpcDataVerbosityLevel {
        utxo_entry_verbosity: (|level| {
            RpcUtxoEntryVerbosity::from(level)
        }),
    }
}

impl_verbosity_from! {
    for RpcTransactionOutputVerbosity, from RpcDataVerbosityLevel {
        include_amount:               (RpcDataVerbosityLevel::Low),
        include_script_public_key:    (RpcDataVerbosityLevel::Low),
        verbose_data_verbosity:       (|level| {
            RpcTransactionOutputVerboseDataVerbosity::from(level)
        }),
    }
}

impl_verbosity_from! {
    for RpcTransactionOutputVerboseDataVerbosity, from RpcDataVerbosityLevel {
        include_script_public_key_type:       (RpcDataVerbosityLevel::Low),
        include_script_public_key_address:    (RpcDataVerbosityLevel::Low),
    }
}

impl_verbosity_from! {
    for RpcTransactionVerbosity, from RpcDataVerbosityLevel {
        include_payload:           (RpcDataVerbosityLevel::High),
        include_mass:              (RpcDataVerbosityLevel::High),
        include_version:           (RpcDataVerbosityLevel::Full),
        include_lock_time:         (RpcDataVerbosityLevel::Full),
        include_subnetwork_id:     (RpcDataVerbosityLevel::Full),
        include_gas:               (RpcDataVerbosityLevel::Full),
        input_verbosity:           (|level| {
            RpcTransactionInputVerbosity::from(level)
        }),
        output_verbosity:          (|level| {
            RpcTransactionOutputVerbosity::from(level)
        }),
        verbose_data_verbosity:    (|level| {
            RpcTransactionVerboseDataVerbosity::from(level)
        }),
    }
}

impl_verbosity_from! {
    for RpcTransactionVerboseDataVerbosity, from RpcDataVerbosityLevel {
        include_transaction_id: (RpcDataVerbosityLevel::Low),
        include_compute_mass:   (RpcDataVerbosityLevel::High),
        include_block_hash:     (RpcDataVerbosityLevel::Low),
        include_block_time:     (RpcDataVerbosityLevel::Low),
        include_hash:           (RpcDataVerbosityLevel::Low),
    }
}

impl_verbosity_from! {
    for RpcAcceptanceDataVerbosity, from RpcDataVerbosityLevel {
        accepting_chain_header_verbosity: (|level| {
            RpcHeaderVerbosity::from(level)
        }),
        mergeset_block_acceptance_data_verbosity: (|level| {
            RpcMergesetBlockAcceptanceDataVerbosity::from(level)
        }),
    }
}

impl_verbosity_from! {
    for RpcMergesetBlockAcceptanceDataVerbosity, from RpcDataVerbosityLevel {
        merged_header_verbosity: (|level| {
            RpcHeaderVerbosity::from(level)
        }),
        accepted_transactions_verbosity: (|level| {
            RpcTransactionVerbosity::from(level)
        }),
    }
}
