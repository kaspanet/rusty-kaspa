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
                let __lvl: u8 = level as u8;
                Self {
                    $(
                        $field: impl_verbosity_from!(@eval __lvl, level, $level, $handler),
                    )*
                    ..::core::default::Default::default()
                }
            }
        }
    };

    // (none) -> None
    (@eval $lvl:ident, $lev:ident, $levty:ty, (none)) => {
        ::core::option::Option::None
    };
    // (|lvl| expr)
    (@eval $lvl:ident, $lev:ident, $levty:ty, (| $v:ident | $e:expr)) => {
        (|$v: u8| ::core::option::Option::Some($e))($lvl)
    };
    // (|lvl, level| expr)
    (@eval $lvl:ident, $lev:ident, $levty:ty, (| $v:ident , $L:ident | $e:expr)) => {
        (|$v: u8, $L: $levty| ::core::option::Option::Some($e))($lvl, $lev)
    };
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
            0 => Ok(Self::Low),
            1 => Ok(Self::Medium),
            2 => Ok(Self::High),
            3 => Ok(Self::Full),
            _ => Err(RpcError::NotImplemented),
        }
    }
}

impl_verbosity_from! {
    for RpcHeaderVerbosity, from RpcDataVerbosityLevel {
        include_hash:                    (|_lvl| true),
        include_version:                 (|lvl| lvl >= 1),
        include_parents_by_level:        (|lvl| lvl >= 2),
        include_hash_merkle_root:        (|lvl| lvl >= 2),
        include_accepted_id_merkle_root: (|lvl| lvl >= 2),
        include_utxo_commitment:         (|lvl| lvl >= 3),
        include_timestamp:               (|lvl| lvl >= 1),
        include_bits:                    (|lvl| lvl >= 1),
        include_nonce:                   (|lvl| lvl >= 1),
        include_daa_score:               (|lvl| lvl >= 1),
        include_blue_work:               (|lvl| lvl >= 1),
        include_blue_score:              (|lvl| lvl >= 1),
        include_pruning_point:           (|lvl| lvl >= 3),
    }
}
impl_verbosity_from! {
    for RpcUtxoEntryVerboseDataVerbosity, from RpcDataVerbosityLevel {
        include_script_public_key_type:      (|lvl| lvl >= 1),
        include_script_public_key_address:   (|lvl| lvl >= 1),
    }
}
impl_verbosity_from! {
    for RpcUtxoEntryVerbosity, from RpcDataVerbosityLevel {
        include_amount:            (|lvl| (lvl >= 2)),
        include_script_public_key: (|lvl| (lvl >= 2)),
        include_block_daa_score:   (|lvl| (lvl >= 3)),
        include_is_coinbase:       (|lvl| (lvl >= 2)),
        verbose_data_verbosity:    (|_lvl, level| {
            RpcUtxoEntryVerboseDataVerbosity::from(level)
        }),
    }
}
impl_verbosity_from! {
    for RpcTransactionInputVerbosity, from RpcDataVerbosityLevel {
        include_signature_script :   (|lvl| lvl >= 1),
        include_sequence:            (|lvl| lvl >= 2),
        include_sig_op_count:        (|lvl| lvl >= 2),
        verbose_data_verbosity:      (|_lvl, level| {
            RpcTransactionInputVerboseDataVerbosity::from(level)
        }),
    }
}
impl_verbosity_from! {
    for RpcTransactionInputVerboseDataVerbosity, from RpcDataVerbosityLevel {
        utxo_entry_verbosity:      (|_lvl, level| {
            RpcUtxoEntryVerbosity::from(level)
        }),
    }
}
impl_verbosity_from! {
    for RpcTransactionOutputVerbosity, from RpcDataVerbosityLevel {
        include_amount:               (|lvl| lvl >= 1),
        include_script_public_key:    (|lvl| lvl >= 1),
        verbose_data_verbosity :      (|_lvl, level| {
            RpcTransactionOutputVerboseDataVerbosity::from(level)
        }),
    }
}
impl_verbosity_from! {
    for RpcTransactionOutputVerboseDataVerbosity, from RpcDataVerbosityLevel {
        include_script_public_key_type:       (|lvl| lvl >= 1),
        include_script_public_key_address:    (|lvl| lvl >= 1),
    }
}
impl_verbosity_from! {
    for RpcTransactionVerbosity, from RpcDataVerbosityLevel {
        include_version:           (|lvl| lvl >= 3),
        input_verbosity:           (|_lvl, level| {
            RpcTransactionInputVerbosity::from(level)
        }),
        output_verbosity:          (|_lvl, level| {
            RpcTransactionOutputVerbosity::from(level)
        }),
        include_lock_time:         (|lvl| lvl >= 4),
        include_subnetwork_id:     (|lvl| lvl >= 4),
        include_gas:               (|lvl| lvl >= 3),
        include_payload:           (|lvl| lvl >= 2),
        include_mass:              (|lvl| lvl >= 2),
        verbose_data_verbosity:    (|_lvl, level| {
            RpcTransactionVerboseDataVerbosity::from(level)
        }),
    }
}
impl_verbosity_from! {
    for RpcTransactionVerboseDataVerbosity, from RpcDataVerbosityLevel {
        include_transaction_id: (|lvl| lvl >= 1),
        include_compute_mass: (|lvl| lvl >= 2),
        include_block_hash: (|lvl| lvl >= 1),
        include_block_time: (|lvl| lvl >= 1),
    }
}
impl_verbosity_from! {
    for RpcAcceptanceDataVerbosity, from RpcDataVerbosityLevel {
        accepting_chain_header_verbosity: (|_lvl, level| {
            RpcHeaderVerbosity::from(level)
        }),
        mergeset_block_acceptance_data_verbosity: (|_lvl, level| {
            RpcMergesetBlockAcceptanceDataVerbosity::from(level)
        }),
    }
}
impl_verbosity_from! {
    for RpcMergesetBlockAcceptanceDataVerbosity, from RpcDataVerbosityLevel {
        merged_header_verbosity: (|_lvl, level| {
            RpcHeaderVerbosity::from(level)
        }),
        accepted_transactions_verbosity: (|_lvl, level| {
            RpcTransactionVerbosity::from(level)
        }),
    }
}
